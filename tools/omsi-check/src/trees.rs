//! `--only trees`: a read-only inventory of the tree and plant objects placed on maps.

use omsi_scenery::sco::SceneryObject;
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Kind {
    Tree,
    PlantModel,
    Backdrop,
    Other,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Tree => "tree",
            Kind::PlantModel => "plant",
            Kind::Backdrop => "backdrop",
            Kind::Other => "other",
        }
    }
}

// the word lists of the game's `vegetation_give_of` (omsi-app, scene/terrain_paint.rs)
const GROUP_WORDS: [&str; 17] = ["tree", "baum", "bäume", "baeume", "deciduous", "conifer", "shrub", "bush", "busch", "strauch", "hecke", "hedge", "plant", "pflanz", "arbor", "vegetation", "forest"];
const NAME_WORDS: [&str; 15] = ["tree", "trees", "baum", "shrub", "shrubbery", "bush", "busch", "strauch", "hecke", "hedge", "arbor", "chestnut", "kastanie", "palm", "plant"];

fn kind_of(sco: &SceneryObject) -> Kind {
    if sco.tree.is_some() {
        return Kind::Tree;
    }
    let groups: Vec<String> = sco.groups.iter().map(|g| g.trim().to_lowercase()).filter(|g| !g.is_empty()).collect();
    if groups.iter().any(|g| g.contains("backdrop")) {
        return Kind::Backdrop;
    }
    let stem = sco.path.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    let plant = if groups.is_empty() {
        stem.split(|c: char| !c.is_alphabetic()).any(|w| NAME_WORDS.contains(&w))
    } else {
        groups.iter().any(|g| g.split(|c: char| !c.is_alphabetic()).any(|word| GROUP_WORDS.iter().any(|w| word.starts_with(w))))
    };
    if plant {
        Kind::PlantModel
    } else {
        Kind::Other
    }
}

/// The texture, height and ratio the game gives a placed tree (`tree_of` in omsi-app).
fn effective(o: &omsi_map::tile::MapObject, tree: &(String, f32, f32, f32, f32)) -> (String, f64, f64) {
    let (tex, h_a, h_b, r_a, r_b) = tree;
    let texture = o.extra.first().cloned().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| tex.clone());
    let mid_h = ((h_a + h_b) * 0.5) as f64;
    let mid_r = ((r_a + r_b) * 0.5) as f64;
    let height = o.extra.get(1).map(|s| omsi_cfg::parse_f64(s)).filter(|h| *h > 0.0).unwrap_or(if mid_h > 0.0 { mid_h } else { 10.0 });
    let ratio = o.extra.get(2).map(|s| omsi_cfg::parse_f64(s)).filter(|r| *r > 0.0).unwrap_or(if mid_r > 0.0 { mid_r } else { 1.0 });
    (texture, height, ratio)
}

struct Agg {
    kind: Kind,
    display: String,
    instances: usize,
    label_textures: BTreeMap<String, usize>,
    height: (f64, f64, f64),
    ratio: (f64, f64, f64),
    groups: String,
    sco_tree: Option<(String, f32, f32, f32, f32)>,
}

impl Agg {
    fn textures_sorted(&self) -> Vec<(&String, &usize)> {
        let mut v: Vec<_> = self.label_textures.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        v
    }
}

fn csv(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

pub fn inventory(root: &Path, out: Option<&Path>, verbose: bool) {
    // the content roots include the installation; the game searches them in this order
    let mut roots = omsi_cfg::content_roots();
    if !roots.contains(&root.to_path_buf()) {
        roots.push(root.to_path_buf());
    }
    let mut globals: Vec<PathBuf> = Vec::new();
    for r in &roots {
        for p in crate::files_with_ext(r, &["cfg"]) {
            if p.file_name().map(|f| f.eq_ignore_ascii_case("global.cfg")).unwrap_or(false) && !globals.contains(&p) {
                globals.push(p);
            }
        }
    }
    let mut scos: HashMap<String, Option<SceneryObject>> = HashMap::new();
    let mut csv_out = String::from("map,kind,sco,instances,textures,height_min,height_max,height_mean,ratio_mean,sco_tree,groups\n");
    globals.sort();
    for g in &globals {
        let Ok(gc) = omsi_map::GlobalCfg::load(g) else { continue };
        let dir = gc.dir().to_path_buf();
        let failed = std::sync::atomic::AtomicUsize::new(0);
        let objects: Vec<omsi_map::tile::MapObject> = gc
            .tiles
            .par_iter()
            .flat_map(|t| {
                let p = omsi_cfg::resolve_path(&dir, &t.file);
                match p.exists().then(|| omsi_map::Tile::load(&p)) {
                    Some(Ok(tile)) => tile.objects.into_iter().chain(tile.attach_objects).filter(|o| o.file.to_ascii_lowercase().ends_with(".sco")).collect(),
                    Some(Err(e)) => {
                        eprintln!("{}: {e}", p.display());
                        failed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        Vec::new()
                    }
                    None => Vec::new(),
                }
            })
            .collect();
        // every distinct object is loaded once, across maps
        for o in &objects {
            scos.entry(o.file.to_ascii_lowercase()).or_insert_with(|| roots.iter().map(|r| omsi_cfg::resolve_path(r, &o.file)).find(|p| omsi_cfg::vfs::is_file(p)).and_then(|p| SceneryObject::load(&p).ok()));
        }
        let mut unresolved = 0;
        let mut aggs: BTreeMap<String, Agg> = BTreeMap::new();
        for o in &objects {
            let Some(Some(sco)) = scos.get(&o.file.to_ascii_lowercase()) else {
                unresolved += 1;
                continue;
            };
            let kind = kind_of(sco);
            if kind == Kind::Other {
                continue;
            }
            let a = aggs.entry(o.file.replace('/', "\\").to_lowercase()).or_insert_with(|| Agg {
                kind,
                display: o.file.replace('/', "\\"),
                instances: 0,
                label_textures: BTreeMap::new(),
                height: (f64::MAX, f64::MIN, 0.0),
                ratio: (f64::MAX, f64::MIN, 0.0),
                groups: sco.groups.join("|"),
                sco_tree: sco.tree.clone(),
            });
            a.instances += 1;
            if let Some(t) = &sco.tree {
                let (tex, h, r) = effective(o, t);
                *a.label_textures.entry(tex.to_lowercase()).or_default() += 1;
                a.height = (a.height.0.min(h), a.height.1.max(h), a.height.2 + h);
                a.ratio = (a.ratio.0.min(r), a.ratio.1.max(r), a.ratio.2 + r);
            }
        }
        let failed = failed.into_inner();
        let failed_note = if failed > 0 { format!(", {failed} tiles failed") } else { String::new() };
        let count = |k: Kind| aggs.values().filter(move |a| a.kind == k);
        println!(
            "map {}: {} tiles, {} objects, {} [tree] instances ({} types), {} plant-model instances ({} types), {} backdrop instances, {} unresolved{}",
            gc.name,
            gc.tiles.len(),
            objects.len(),
            count(Kind::Tree).map(|a| a.instances).sum::<usize>(),
            count(Kind::Tree).count(),
            count(Kind::PlantModel).map(|a| a.instances).sum::<usize>(),
            count(Kind::PlantModel).count(),
            count(Kind::Backdrop).map(|a| a.instances).sum::<usize>(),
            unresolved,
            failed_note
        );
        let mut list: Vec<(&String, &Agg)> = aggs.iter().filter(|(_, a)| a.kind != Kind::Backdrop).collect();
        list.sort_by(|a, b| b.1.instances.cmp(&a.1.instances).then(a.0.cmp(b.0)));
        for (_, a) in list.iter().take(if verbose { usize::MAX } else { 10 }) {
            let path = &a.display;
            let tex = a.textures_sorted().first().map(|(t, c)| format!("{t}({c})")).unwrap_or_default();
            let h = if a.kind == Kind::Tree { format!("{:.1}-{:.1}", a.height.0, a.height.1) } else { "-".into() };
            println!("  {}  {}  {}  h {}  tex {}", a.instances, a.kind.name(), path, h, tex);
        }
        for (_, a) in &aggs {
            let path = &a.display;
            let n = a.instances as f64;
            let textures = a.textures_sorted().iter().map(|(t, c)| format!("{t}:{c}")).collect::<Vec<_>>().join(";");
            let (hmin, hmax, hmean, rmean) = if a.kind == Kind::Tree { (format!("{:.2}", a.height.0), format!("{:.2}", a.height.1), format!("{:.2}", a.height.2 / n), format!("{:.2}", a.ratio.2 / n)) } else { Default::default() };
            let sco_tree = a.sco_tree.as_ref().map(|(t, ha, hb, ra, rb)| format!("{t} h {:.2}-{:.2} r {:.2}-{:.2}", ha.min(*hb), ha.max(*hb), ra.min(*rb), ra.max(*rb))).unwrap_or_default();
            let _ = writeln!(csv_out, "{},{},{},{},{},{hmin},{hmax},{hmean},{rmean},{},{}", csv(&gc.name), a.kind.name(), csv(path), a.instances, csv(&textures), csv(&sco_tree), csv(&a.groups));
        }
    }
    if let Some(out) = out {
        if let Err(e) = std::fs::write(out, csv_out) {
            eprintln!("{}: {e}", out.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sco(groups: &[&str], file: &str, tree: bool) -> SceneryObject {
        SceneryObject {
            path: file.into(),
            groups: groups.iter().map(|g| g.to_string()).collect(),
            tree: tree.then(|| ("t.bmp".to_string(), 5.0, 8.0, 0.5, 1.0)),
            ..Default::default()
        }
    }

    #[test]
    fn kinds() {
        assert_eq!(kind_of(&sco(&["German Street Side"], "a/pole.sco", false)), Kind::Other);
        assert_eq!(kind_of(&sco(&["Trees LQ"], "a/x.sco", false)), Kind::PlantModel);
        assert_eq!(kind_of(&sco(&[], "a/trees_chestnut01.sco", false)), Kind::PlantModel);
        assert_eq!(kind_of(&sco(&["Forest backdrop"], "a/x.sco", false)), Kind::Backdrop);
        assert_eq!(kind_of(&sco(&[], "a/x.sco", true)), Kind::Tree);
    }
}
