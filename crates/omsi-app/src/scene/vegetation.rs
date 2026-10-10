//! Tree and plant classes for the `OMSI_DEBUG_TREES` log (nothing is drawn differently), and
//! under `OMSI_TREES_GENERATE` the shared tree variants, generated on the background pool.
use super::*;
use omsi_vegetation::classify::{Archetype, Class, Signals};
use omsi_vegetation::cache::{self, VariantKey};
use omsi_vegetation::generator::{Lod, TreeMesh};
use omsi_vegetation::overrides::{classify_with, Overrides};
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::OnceLock;

// (read by the renderer in PR9; only the tests read the meshes for now)
#[allow(dead_code)]
pub(crate) enum VariantSlot {
    Pending,
    Ready(Arc<TreeMesh>),
    Missing,
}

#[derive(Default)]
pub(crate) struct VariantStats {
    pub generated: AtomicU32,
    pub loaded: AtomicU32,
    /// sum of job wall time in microseconds (generate or load, including the store)
    pub job_us: AtomicU64,
    pub max_job_us: AtomicU64,
}

#[derive(Default)]
pub(crate) struct Vegetation {
    overrides: OnceLock<Overrides>,
    classes: Mutex<HashMap<(String, String), Class>>,
    // ponytail: kept for the map's lifetime (bounded: archetypes x VARIANTS x 3 LODs, ~1 MB each); per-tile refcounts if the GPU copies of PR9 need to be freed while driving.
    variants: Arc<Mutex<HashMap<VariantKey, VariantSlot>>>,
    pub(crate) variant_stats: Arc<VariantStats>,
    /// None = the disk cache is off (no home dir); set once, tests set it to None.
    cache_root: OnceLock<Option<PathBuf>>,
}

impl Vegetation {
    /// Rules of every `Vegetation/*.cfg` of the content roots (archives included), read
    /// once, in file-name order (ASCII case-insensitive); warnings go to `log::warn!`.
    fn overrides(&self) -> &Overrides {
        self.overrides.get_or_init(|| {
            let mut files: Vec<PathBuf> = omsi_cfg::read_dir_merged("Vegetation")
                .into_iter()
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("cfg")))
                .collect();
            files.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()));
            let mut o = Overrides::default();
            for f in &files {
                match omsi_cfg::vfs::read(f) {
                    Ok(b) => {
                        for w in o.parse_append(f, &omsi_cfg::decode_text(&b)) {
                            log::warn!("trees: {w}");
                        }
                    }
                    Err(e) => log::warn!("trees: {}: {e}", f.display()),
                }
            }
            log::info!("trees: {} override rules from {} files", o.rules.len(), files.len());
            o
        })
    }

    /// `$HOME` or `$USERPROFILE` + `.openomsi/cache/vegetation`; None if neither is set.
    fn cache_root(&self) -> Option<&Path> {
        self.cache_root
            .get_or_init(|| {
                let home = ["HOME", "USERPROFILE"].iter().find_map(|v| std::env::var_os(v).filter(|h| Path::new(h).is_absolute()))?;
                Some(PathBuf::from(home).join(".openomsi").join("cache").join("vegetation"))
            })
            .as_deref()
    }

    /// Ask for L0, L1 and L2 of `instance_seed`'s variant of `archetype`; each key not yet in
    /// the map gets one job on the background pool. Returns the variant index.
    pub(crate) fn request_variants(&self, archetype: Archetype, instance_seed: u64) -> u32 {
        let v = cache::variant_of(instance_seed);
        let mut to_spawn = Vec::new();
        {
            let mut map = self.variants.lock();
            for lod in [Lod::L0, Lod::L1, Lod::L2] {
                let k = VariantKey { archetype, variant: v, lod };
                if !map.contains_key(&k) {
                    map.insert(k, VariantSlot::Pending);
                    to_spawn.push(k);
                }
            }
        }
        if to_spawn.is_empty() {
            return v;
        }
        let root = self.cache_root().map(Path::to_path_buf);
        for k in to_spawn {
            let (variants, stats, root) = (self.variants.clone(), self.variant_stats.clone(), root.clone());
            crate::threads::background_pool().spawn(move || {
                let t0 = std::time::Instant::now();
                // (a panic in a pool job aborts the process: it ends as a Missing slot instead)
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cache::get_or_generate(root.as_deref(), k)));
                let us = t0.elapsed().as_micros() as u64;
                stats.job_us.fetch_add(us, Ordering::Relaxed);
                stats.max_job_us.fetch_max(us, Ordering::Relaxed);
                let slot = match r {
                    Err(_) => {
                        log::warn!("trees: variant {:?}-{}-{:?} failed: generator panicked", k.archetype, k.variant, k.lod);
                        VariantSlot::Missing
                    }
                    Ok(Some((m, from_disk))) => {
                        let n = if from_disk { &stats.loaded } else { &stats.generated };
                        n.fetch_add(1, Ordering::Relaxed);
                        log::debug!(
                            "trees: variant {:?}-{}-{:?} {} in {:.1} ms ({} triangles)",
                            k.archetype,
                            k.variant,
                            k.lod,
                            if from_disk { "loaded" } else { "generated" },
                            us as f64 / 1000.0,
                            m.triangles()
                        );
                        VariantSlot::Ready(Arc::new(m))
                    }
                    Ok(None) => VariantSlot::Missing,
                };
                variants.lock().insert(k, slot);
            });
        }
        v
    }

    /// One line for the `OMSI_PROFILE` summary; None while no variant was ever requested.
    pub(crate) fn variant_summary(&self) -> Option<String> {
        fn mesh_bytes(m: &omsi_vegetation::generator::Mesh) -> usize {
            m.positions.len() * 12 + m.normals.len() * 12 + m.uvs.len() * 8 + m.attrs.len() * 64 + m.indices.len() * 4
        }
        let (total, pending, missing, bytes) = {
            let map = self.variants.lock();
            let (mut pending, mut missing, mut bytes) = (0, 0, 0);
            for s in map.values() {
                match s {
                    VariantSlot::Pending => pending += 1,
                    VariantSlot::Missing => missing += 1,
                    VariantSlot::Ready(m) => bytes += mesh_bytes(&m.bark) + mesh_bytes(&m.leaves),
                }
            }
            (map.len(), pending, missing, bytes)
        };
        if total == 0 {
            return None;
        }
        let st = &self.variant_stats;
        let (generated, loaded) = (st.generated.load(Ordering::Relaxed), st.loaded.load(Ordering::Relaxed));
        let mean_ms = st.job_us.load(Ordering::Relaxed) as f64 / f64::from((generated + loaded).max(1)) / 1000.0;
        Some(format!(
            "trees: {total} variants ({generated} generated, {loaded} loaded, {pending} pending, {missing} missing), job mean {mean_ms:.1} ms, max {:.1} ms, {:.1} MB in memory",
            st.max_job_us.load(Ordering::Relaxed) as f64 / 1000.0,
            bytes as f64 / 1e6
        ))
    }

    /// The mesh if Ready.
    #[allow(dead_code)]
    pub(crate) fn variant(&self, key: VariantKey) -> Option<Arc<TreeMesh>> {
        match self.variants.lock().get(&key) {
            Some(VariantSlot::Ready(m)) => Some(m.clone()),
            _ => None,
        }
    }

    /// Class of a placed `[tree]`: `texture` = the label texture (before the season look),
    /// `height` = the tree's effective height (m).
    pub(crate) fn tree_class(&self, ot: &ObjectType, texture: &str, height: f64) -> Class {
        let tex = texture.rsplit(['/', '\\']).next().unwrap_or(texture);
        // ponytail: the height of the first instance seen decides the class (it matters only
        // through the 2 m / 3 m thresholds); Phase 4 keys by height band if needed.
        self.class_of(ot, (!tex.is_empty()).then_some(tex), Some(height as f32), true)
    }

    /// Class of a plant model (`vegetation_give_of` is Some): height = z range of all
    /// `ot.meshes` positions (None if no positions), texture None.
    pub(crate) fn plant_class(&self, ot: &ObjectType) -> Class {
        let zs = || ot.meshes.iter().flat_map(|(m, _, _)| m.positions.iter()).map(|p| p.z);
        let height = zs().reduce(f32::max).zip(zs().reduce(f32::min)).map(|(hi, lo)| hi - lo);
        self.class_of(ot, None, height, false)
    }

    fn class_of(&self, ot: &ObjectType, texture: Option<&str>, height: Option<f32>, tree: bool) -> Class {
        let sco = omsi_cfg::root_relative(&ot.sco.path).unwrap_or_else(|| ot.sco.path.clone()).to_string_lossy().into_owned();
        let key = (sco.to_lowercase(), texture.map(str::to_lowercase).unwrap_or_default());
        if let Some(c) = self.classes.lock().get(&key) {
            return *c;
        }
        let o = self.overrides();
        let sig = Signals { tree_section: tree, sco_path: &sco, groups: &ot.sco.groups, texture, height };
        let (c, rule) = classify_with(o, &sig);
        if self.classes.lock().insert(key, c).is_none() {
            let look = if c.archetype == Archetype::None {
                "original look".to_string()
            } else {
                format!("{} {}", c.archetype.name(), c.species.unwrap_or("-"))
            };
            log::info!(
                "trees: {} + {} -> {} {:.2} [{}]{}",
                sco,
                texture.unwrap_or("-"),
                look,
                c.confidence,
                rule.map_or("classifier", |r| r.origin.as_str()),
                if c.archetype != Archetype::None && !c.replace_geometry() { " [texture-only]" } else { "" }
            );
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn keys(a: Archetype, v: u32) -> [VariantKey; 3] {
        [Lod::L0, Lod::L1, Lod::L2].map(|lod| VariantKey { archetype: a, variant: v, lod })
    }

    fn wait(limit: Duration, f: impl Fn() -> bool) {
        let t = Instant::now();
        while !f() && t.elapsed() < limit {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn request_generates_three_lods_once() {
        let v = Vegetation::default();
        v.cache_root.set(None).unwrap();
        let i = v.request_variants(Archetype::Shrub, 12345);
        assert_eq!(v.request_variants(Archetype::Shrub, 12345), i);
        assert_eq!(v.variants.lock().len(), 3);
        let k = keys(Archetype::Shrub, i);
        wait(Duration::from_secs(60), || k.iter().all(|k| v.variant(*k).is_some()));
        let m: Vec<_> = k.iter().map(|k| v.variant(*k).expect("variant ready")).collect();
        assert_eq!(v.variant_stats.generated.load(Ordering::Relaxed), 3);
        assert_eq!(v.variant_stats.loaded.load(Ordering::Relaxed), 0);
        assert!(m[0].triangles() >= m[2].triangles());
    }

    #[test]
    fn variant_summary_counts() {
        let v = Vegetation::default();
        v.cache_root.set(None).unwrap();
        assert!(v.variant_summary().is_none());
        let i = v.request_variants(Archetype::Shrub, 7);
        let k = keys(Archetype::Shrub, i);
        wait(Duration::from_secs(60), || k.iter().all(|k| v.variant(*k).is_some()));
        let s = v.variant_summary().expect("summary after a request");
        assert!(s.starts_with("trees: 3 variants (3 generated, 0 loaded, 0 pending, 0 missing)"), "{s}");
        assert!(s.contains(" MB in memory"), "{s}");
    }

    #[test]
    fn none_archetype_ends_missing() {
        let v = Vegetation::default();
        v.cache_root.set(None).unwrap();
        let i = v.request_variants(Archetype::None, 1);
        let missing = || keys(Archetype::None, i).iter().all(|k| matches!(v.variants.lock().get(k), Some(VariantSlot::Missing)));
        wait(Duration::from_secs(10), missing);
        assert!(missing());
        assert!(keys(Archetype::None, i).iter().all(|k| v.variant(*k).is_none()));
    }
}
