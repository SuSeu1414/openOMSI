//! Tree and plant classes for the `OMSI_DEBUG_TREES` log (nothing is drawn differently).
use super::*;
use omsi_vegetation::classify::{Archetype, Class, Signals};
use omsi_vegetation::overrides::{classify_with, Overrides};

#[derive(Default)]
pub(crate) struct Vegetation {
    overrides: std::sync::OnceLock<Overrides>,
    classes: Mutex<HashMap<(String, String), Class>>,
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
