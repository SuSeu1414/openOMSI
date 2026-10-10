//! Classification of a placed tree or plant: archetype, species and a confidence.
//!
//! Only objects with `replace_geometry()` true are meant to get new geometry; the rest keep
//! the original look.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Archetype {
    BroadleafWide,
    BroadleafNarrow,
    ConiferConical,
    Pine,
    Willow,
    Birch,
    Shrub,
    Hedge,
    None,
}

const ALL: [Archetype; 9] = [
    Archetype::BroadleafWide,
    Archetype::BroadleafNarrow,
    Archetype::ConiferConical,
    Archetype::Pine,
    Archetype::Willow,
    Archetype::Birch,
    Archetype::Shrub,
    Archetype::Hedge,
    Archetype::None,
];

impl Archetype {
    /// snake_case name, e.g. "broadleaf_wide".
    pub fn name(self) -> &'static str {
        match self {
            Archetype::BroadleafWide => "broadleaf_wide",
            Archetype::BroadleafNarrow => "broadleaf_narrow",
            Archetype::ConiferConical => "conifer_conical",
            Archetype::Pine => "pine",
            Archetype::Willow => "willow",
            Archetype::Birch => "birch",
            Archetype::Shrub => "shrub",
            Archetype::Hedge => "hedge",
            Archetype::None => "none",
        }
    }

    /// Inverse of `name`, ASCII case-insensitive; `None` for unknown.
    pub fn from_name(s: &str) -> Option<Archetype> {
        ALL.into_iter().find(|a| a.name().eq_ignore_ascii_case(s))
    }
}

/// What is known about one placed tree or plant. Strings as found (any case, any slashes).
pub struct Signals<'a> {
    /// the .sco has `[tree]`
    pub tree_section: bool,
    /// e.g. "Sceneryobjects\\Trees_MC\\tree_medium_04.sco"
    pub sco_path: &'a str,
    /// `SceneryObject.groups`
    pub groups: &'a [String],
    /// effective texture of the instance (tree) or None
    pub texture: Option<&'a str>,
    /// effective height in m; None = unknown
    pub height: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Class {
    pub archetype: Archetype,
    pub species: Option<&'static str>,
    pub confidence: f32,
}

pub const MIN_CONFIDENCE: f32 = 0.6;

impl Class {
    /// archetype != None && confidence >= MIN_CONFIDENCE
    pub fn replace_geometry(&self) -> bool {
        self.archetype != Archetype::None && self.confidence >= MIN_CONFIDENCE
    }
}

// the word lists of the game's `vegetation_give_of` (omsi-app, scene/terrain_paint.rs)
pub const PLANT_GROUP_WORDS: [&str; 17] = ["tree", "baum", "bäume", "baeume", "deciduous", "conifer", "shrub", "bush", "busch", "strauch", "hecke", "hedge", "plant", "pflanz", "arbor", "vegetation", "forest"];
pub const PLANT_NAME_WORDS: [&str; 15] = ["tree", "trees", "baum", "shrub", "shrubbery", "bush", "busch", "strauch", "hecke", "hedge", "arbor", "chestnut", "kastanie", "palm", "plant"];

/// A plant model: with groups, any group word starts with a group word; without, a word of
/// the file stem is a name word. `stem`: file stem, any case.
pub fn is_plant(groups: &[String], stem: &str) -> bool {
    let groups: Vec<String> = groups.iter().map(|g| g.trim().to_lowercase()).filter(|g| !g.is_empty()).collect();
    if groups.is_empty() {
        stem.to_lowercase().split(|c: char| !c.is_alphabetic()).any(|w| PLANT_NAME_WORDS.contains(&w))
    } else {
        groups.iter().any(|g| g.split(|c: char| !c.is_alphabetic()).any(|word| PLANT_GROUP_WORDS.iter().any(|w| word.starts_with(w))))
    }
}

const GROUND_COVER: [&str; 11] = ["gras", "gestruepp", "blume", "farn", "fern", "unkraut", "flower", "weed", "moos", "schilf", "bodendecker"];

use Archetype as A;

/// (key, archetype, species); order is the priority within a source.
const SPECIES: &[(&str, Archetype, Option<&str>)] = &[
    ("birke", A::Birch, Some("birch")),
    ("birch", A::Birch, Some("birch")),
    ("pappel", A::BroadleafNarrow, Some("poplar")),
    ("poplar", A::BroadleafNarrow, Some("poplar")),
    ("kiefer", A::Pine, Some("pine")),
    ("pine", A::Pine, Some("pine")),
    ("weide", A::Willow, Some("willow")),
    ("willow", A::Willow, Some("willow")),
    ("fichte", A::ConiferConical, Some("spruce")),
    ("spruce", A::ConiferConical, Some("spruce")),
    ("tanne", A::ConiferConical, Some("fir")),
    ("fir", A::ConiferConical, Some("fir")),
    ("laerche", A::ConiferConical, Some("larch")),
    ("larch", A::ConiferConical, Some("larch")),
    ("zypresse", A::ConiferConical, Some("cypress")),
    ("cypress", A::ConiferConical, Some("cypress")),
    ("thuja", A::ConiferConical, Some("thuja")),
    ("eiche", A::BroadleafWide, Some("oak")),
    ("oak", A::BroadleafWide, Some("oak")),
    ("buche", A::BroadleafWide, Some("beech")),
    ("beech", A::BroadleafWide, Some("beech")),
    ("linde", A::BroadleafWide, Some("linden")),
    ("linden", A::BroadleafWide, Some("linden")),
    ("ahorn", A::BroadleafWide, Some("maple")),
    ("maple", A::BroadleafWide, Some("maple")),
    ("kastanie", A::BroadleafWide, Some("chestnut")),
    ("chestnut", A::BroadleafWide, Some("chestnut")),
    ("platane", A::BroadleafWide, Some("plane")),
    ("hecke", A::Hedge, Some("hedge")),
    ("hedge", A::Hedge, Some("hedge")),
    ("strauch", A::Shrub, Some("shrub")),
    ("gebuesch", A::Shrub, Some("shrub")),
    ("busch", A::Shrub, Some("shrub")),
    ("shrub", A::Shrub, Some("shrub")),
    ("bush", A::Shrub, Some("shrub")),
    ("conifer", A::ConiferConical, None),
    ("nadel", A::ConiferConical, None),
    ("deciduous", A::BroadleafWide, None),
    ("laub", A::BroadleafWide, None),
];

/// Lowercase words of `s`: umlauts spelled out, everything non-alphabetic separates.
fn norm(s: &str) -> Vec<String> {
    let mut t = String::with_capacity(s.len());
    for c in s.to_lowercase().chars() {
        match c {
            'ä' => t.push_str("ae"),
            'ö' => t.push_str("oe"),
            'ü' => t.push_str("ue"),
            'ß' => t.push_str("ss"),
            c => t.push(c),
        }
    }
    t.split(|c: char| !c.is_ascii_alphabetic()).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

fn stem(path: &str) -> &str {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(s, _)| s)
}

fn matches(word: &str, key: &str) -> bool {
    if key.len() >= 5 {
        word.contains(key)
    } else {
        word.starts_with(key)
    }
}

pub fn classify(s: &Signals) -> Class {
    let none = Class { archetype: Archetype::None, species: None, confidence: 0.0 };
    let sources = [norm(s.texture.map(stem).unwrap_or("")), norm(stem(s.sco_path)), norm(&s.groups.join(" "))];
    if sources.iter().flatten().any(|w| GROUND_COVER.iter().any(|k| matches(w, k))) {
        return none;
    }
    if s.height.is_some_and(|h| h < 2.0) {
        return none;
    }
    let species = sources.iter().find_map(|words| SPECIES.iter().find(|(k, ..)| words.iter().any(|w| matches(w, k))));
    let plant = is_plant(s.groups, stem(s.sco_path));
    let mut conf = 0.0;
    if s.tree_section {
        conf += 0.5;
    }
    if species.is_some() {
        conf += 0.3;
    }
    if plant {
        conf += 0.2;
    }
    if s.height.is_some_and(|h| h >= 3.0) {
        conf += 0.1;
    }
    let confidence = f32::min(conf, 1.0);
    if !s.tree_section && species.is_none() && !plant {
        return none;
    }
    let mut archetype = species.map_or(Archetype::BroadleafWide, |e| e.1);
    if s.height.is_some_and(|h| h < 3.0) && matches!(archetype, Archetype::BroadleafWide | Archetype::BroadleafNarrow | Archetype::Birch | Archetype::Willow) {
        archetype = Archetype::Shrub;
    }
    Class { archetype, species: species.and_then(|e| e.2), confidence }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Archetype::{BroadleafNarrow, BroadleafWide, Birch, ConiferConical, Pine, Shrub, Willow};

    const D: &str = "Sceneryobjects\\Trees_MC\\";

    /// (tree_section, path, groups, texture, height) -> (archetype, species, confidence)
    type Row = (bool, String, Vec<String>, Option<&'static str>, Option<f32>, Archetype, Option<&'static str>, Option<f32>);

    fn rows() -> Vec<Row> {
        let g = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let p = |f: &str| format!("{D}{f}");
        vec![
            (true, p("tree_medium_04.sco"), g(&[]), Some("kastanie01.tga"), Some(14.35), BroadleafWide, Some("chestnut"), Some(1.0)),
            (true, p("Birke01.sco"), g(&[]), Some("Birke01.dds"), Some(15.5), Birch, Some("birch"), Some(0.9)),
            (true, p("tree_medium_02.sco"), g(&[]), Some("tree_medium_02.tga"), Some(12.0), BroadleafWide, None, Some(0.8)),
            (true, p("Blumen_rosa_1.sco"), g(&[]), Some("blumen_rosa.tga"), Some(1.0), A::None, None, Some(0.0)),
            (true, p("Bodengestruepp03.sco"), g(&[]), Some("x.tga"), Some(4.0), A::None, None, Some(0.0)),
            (true, p("Ebereschengestruepp.sco"), g(&[]), None, None, A::None, None, Some(0.0)),
            (true, p("Trauerweide.sco"), g(&[]), Some("trauerweide.tga"), Some(10.0), Willow, Some("willow"), None),
            (true, p("x.sco"), g(&[]), Some("Schwarzkiefer.tga"), Some(18.0), Pine, Some("pine"), Some(0.9)),
            (true, p("Pappel01.sco"), g(&[]), Some("Pappel01.tga"), Some(25.0), BroadleafNarrow, Some("poplar"), None),
            (true, p("Fichte_gross.sco"), g(&[]), None, None, ConiferConical, Some("spruce"), None),
            (false, "Sceneryobjects\\Ruede\\trees_chestnut01.sco".into(), g(&[]), None, None, BroadleafWide, Some("chestnut"), Some(0.5)),
            (false, p("shrubbery_03_20.sco"), g(&[]), None, Some(1.5), A::None, None, Some(0.0)),
            (false, p("shrubbery_03_20.sco"), g(&[]), None, Some(2.5), Shrub, Some("shrub"), Some(0.5)),
            (false, p("Laterne01.sco"), g(&["German Street Side"]), None, None, A::None, None, Some(0.0)),
            (true, p("x.sco"), g(&[]), Some("x.tga"), Some(2.5), Shrub, None, Some(0.5)),
            (true, p("Rotbuche.sco"), g(&["Bäume Laubbäume"]), Some("rotbuche.tga"), Some(20.0), BroadleafWide, Some("beech"), Some(1.0)),
        ]
    }

    #[test]
    fn table() {
        for (i, (tree, path, groups, tex, h, arch, species, conf)) in rows().into_iter().enumerate() {
            let c = classify(&Signals { tree_section: tree, sco_path: &path, groups: &groups, texture: tex, height: h });
            assert_eq!((c.archetype, c.species), (arch, species), "row {}", i + 1);
            if let Some(x) = conf {
                assert!((c.confidence - x).abs() < 1e-6, "row {}: {}", i + 1, c.confidence);
            }
        }
    }

    #[test]
    fn replace_geometry_threshold() {
        let c = classify(&Signals { tree_section: false, sco_path: "a\\trees_chestnut01.sco", groups: &[], texture: None, height: None });
        assert!(!c.replace_geometry());
        let c = classify(&Signals { tree_section: true, sco_path: "a\\x.sco", groups: &[], texture: Some("kastanie.tga"), height: Some(10.0) });
        assert!(c.replace_geometry());
    }

    #[test]
    fn names_round_trip() {
        for a in ALL {
            assert_eq!(Archetype::from_name(a.name()), Some(a));
        }
        assert_eq!(Archetype::from_name("BROADLEAF_WIDE"), Some(BroadleafWide));
        assert_eq!(Archetype::from_name("x"), Option::None);
    }
}
