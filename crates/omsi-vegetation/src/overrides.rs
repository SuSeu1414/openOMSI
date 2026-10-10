//! User and mod-pack corrections of the classifier: rules read from `Vegetation/*.cfg`.
//! Pure data: the caller finds the files and passes their text.

use crate::classify::{classify, is_known_species, species_static, Archetype, Class, Signals};
use omsi_cfg::CfgFile;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Sco,
    Texture,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub target: Target,
    /// as written (for messages)
    pub pattern: String,
    /// `Archetype::None` = keep the original look
    pub archetype: Archetype,
    /// lowercase; None when "-" or empty
    pub species: Option<String>,
    /// "<file path>:<line>" of the `[rule]` keyword
    pub origin: String,
}

#[derive(Clone, Debug, Default)]
pub struct Overrides {
    pub rules: Vec<Rule>,
}

impl Overrides {
    /// Parse one file's text and append its rules (file order kept). Returns warnings for
    /// skipped rules, each "<path>:<line>: <reason>".
    pub fn parse_append(&mut self, path: &Path, text: &str) -> Vec<String> {
        let file = CfgFile::from_str(path, text);
        let mut r = file.reader();
        let mut warnings = Vec::new();
        while let Some(k) = r.next_keyword() {
            if k != "rule" {
                continue;
            }
            let origin = format!("{}:{}", path.display(), r.block_line());
            let mut w = [""; 4];
            let mut truncated = false;
            for slot in &mut w {
                let at = r.pos();
                *slot = r.word();
                if slot.starts_with('[') {
                    r.seek(at); // leave the keyword line for the next block
                    truncated = true;
                    break;
                }
            }
            if truncated {
                warnings.push(format!("{origin}: missing parameters"));
                continue;
            }
            let [t, pat, arch, sp] = w;
            let target = if t.eq_ignore_ascii_case("sco") {
                Target::Sco
            } else if t.eq_ignore_ascii_case("texture") {
                Target::Texture
            } else {
                warnings.push(format!("{origin}: unknown target '{t}'"));
                continue;
            };
            if pat.is_empty() {
                warnings.push(format!("{origin}: empty pattern"));
                continue;
            }
            let Some(archetype) = Archetype::from_name(arch) else {
                warnings.push(format!("{origin}: unknown archetype '{arch}'"));
                continue;
            };
            let species = if sp.is_empty() || sp == "-" {
                None
            } else if is_known_species(sp) {
                Some(sp.to_ascii_lowercase())
            } else {
                warnings.push(format!("{origin}: unknown species '{sp}'"));
                continue;
            };
            self.rules.push(Rule {
                target,
                pattern: pat.to_string(),
                archetype,
                species,
                origin,
            });
        }
        warnings
    }

    /// First rule (in append order) whose pattern matches: a `Sco` rule against `sco_path`,
    /// a `Texture` rule against the file name of `texture` (folders stripped; never matches
    /// when `texture` is None).
    pub fn find(&self, sco_path: &str, texture: Option<&str>) -> Option<&Rule> {
        let tex = texture.map(|t| t.rsplit(['/', '\\']).next().unwrap_or(t));
        self.rules.iter().find(|r| match r.target {
            Target::Sco => glob_match(&r.pattern, sco_path),
            Target::Texture => tex.is_some_and(|t| glob_match(&r.pattern, t)),
        })
    }
}

/// The class of a tree: the first matching rule (taken as certain, confidence 1.0;
/// archetype None keeps the original look), else the classifier.
pub fn classify_with<'r>(o: &'r Overrides, s: &Signals) -> (Class, Option<&'r Rule>) {
    match o.find(s.sco_path, s.texture) {
        Some(r) => {
            let confidence = if r.archetype == Archetype::None {
                0.0
            } else {
                1.0
            };
            (
                Class {
                    archetype: r.archetype,
                    species: r.species.as_deref().and_then(species_static),
                    confidence,
                },
                Some(r),
            )
        }
        None => (classify(s), None),
    }
}

fn norm(s: &str) -> Vec<char> {
    s.chars()
        .map(|c| {
            if c == '/' {
                '\\'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

/// `*` = any run of characters (separators included), `?` = one character; ASCII
/// case-insensitive; `/` and `\` are the same character. Whole-string match.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let (p, t) = (norm(pattern), norm(text));
    let (mut pi, mut ti) = (0, 0);
    // (pattern index after the last '*', text index it currently absorbs up to)
    let mut back: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            back = Some((pi + 1, ti));
            pi += 1;
        } else if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if let Some((bp, bt)) = back {
            back = Some((bp, bt + 1));
            pi = bp;
            ti = bt + 1;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = "[rule]\nsco\nSceneryobjects\\Trees_MC\\tree_medium_0*.sco\nbroadleaf_wide\nlinden\n\n[rule]\ntexture\nBlumen*.tga\nnone\n-\n";

    #[test]
    fn glob() {
        let g = glob_match;
        assert!(g(
            "*\\tree_medium_0*.sco",
            "Sceneryobjects\\Trees_MC\\tree_medium_04.sco"
        ));
        assert!(g(
            "*/tree_medium_0*.sco",
            "Sceneryobjects\\Trees_MC\\tree_medium_04.sco"
        ));
        assert!(g("TREE_MEDIUM_04.SCO", "tree_medium_04.sco"));
        assert!(g("tree_?.sco", "tree_1.sco"));
        assert!(!g("tree_?.sco", "tree_12.sco"));
        assert!(g("*", ""));
        assert!(g("a*b*c", "axxbyyc"));
        assert!(!g("a*b*c", "axxbyy"));
        assert!(!g("", "a"));
    }

    #[test]
    fn parse_example() {
        let mut o = Overrides::default();
        let w = o.parse_append(Path::new("a.cfg"), EXAMPLE);
        assert!(w.is_empty());
        assert_eq!(o.rules.len(), 2);
        let (a, b) = (&o.rules[0], &o.rules[1]);
        assert_eq!(
            (a.target, a.archetype, a.species.as_deref()),
            (Target::Sco, Archetype::BroadleafWide, Some("linden"))
        );
        assert_eq!(
            (b.target, b.archetype, b.species.as_deref()),
            (Target::Texture, Archetype::None, None)
        );
        assert_eq!(b.pattern, "Blumen*.tga");
        assert!(a.origin.ends_with(":1") && b.origin.ends_with(":7"));
    }

    #[test]
    fn parse_bad_rules() {
        let text = "[rule]\nbogus\nx\npine\n-\n[rule]\nsco\n\npine\n-\n[rule]\nsco\nx\ntree\n-\n[rule]\nsco\nx\npine\npalm\n[rule]\nsco\nok\npine\nPine\n";
        let mut o = Overrides::default();
        let w = o.parse_append(Path::new("a.cfg"), text);
        assert_eq!(w.len(), 4, "{w:?}");
        for (i, line) in [1, 6, 11, 16].iter().enumerate() {
            assert!(w[i].contains(&format!("a.cfg:{line}: ")), "{}", w[i]);
        }
        assert_eq!(o.rules.len(), 1);
        assert_eq!(o.rules[0].species.as_deref(), Some("pine"));
    }

    #[test]
    fn parse_truncated_rule() {
        let mut o = Overrides::default();
        let w = o.parse_append(
            Path::new("a.cfg"),
            "[rule]
sco
[rule]
texture
x*.tga
none
-",
        );
        assert_eq!(o.rules.len(), 1);
        assert_eq!(o.rules[0].target, Target::Texture);
        assert_eq!(w.len(), 1);
        assert!(w[0].contains(":1:"), "{w:?}");
    }

    #[test]
    fn parse_ignores_other_blocks() {
        let mut o = Overrides::default();
        let w = o.parse_append(
            Path::new("a.cfg"),
            &format!("comment\n[other]\nx\n{EXAMPLE}trailing text\n"),
        );
        assert!(w.is_empty());
        assert_eq!(o.rules.len(), 2);
    }

    #[test]
    fn find_rules() {
        let mut o = Overrides::default();
        o.parse_append(
            Path::new("a.cfg"),
            "[rule]\ntexture\nBlumen*.tga\nnone\n-\n",
        );
        o.parse_append(
            Path::new("b.cfg"),
            "[rule]\ntexture\n*.tga\nshrub\n-\n[rule]\nsco\n*.sco\npine\npine\n",
        );
        let r = o
            .find("x.sco", Some("Texture\\Trees\\Blumen_rosa.tga"))
            .unwrap();
        assert_eq!(r.archetype, Archetype::None);
        assert_eq!(
            o.find("x.sco", Some("other.tga")).unwrap().archetype,
            Archetype::Shrub
        );
        assert_eq!(o.find("x.sco", None).unwrap().archetype, Archetype::Pine);
        assert!(o.find("x.txt", None).is_none());
    }

    #[test]
    fn classify_with_rules() {
        let sig = |sco| Signals {
            tree_section: true,
            sco_path: sco,
            groups: &[],
            texture: Some("Birke01.dds"),
            height: Some(15.0),
        };
        let mut o = Overrides::default();
        let (plain, rule) = classify_with(&o, &sig("a/x.sco"));
        assert!(rule.is_none());
        assert_eq!(plain, classify(&sig("a/x.sco")));
        assert_eq!(plain.archetype, Archetype::Birch);
        o.parse_append(
            Path::new("a.cfg"),
            "[rule]
sco
*/x.sco
pine
pine
[rule]
sco
*/y.sco
none
-
",
        );
        let (c, r) = classify_with(&o, &sig("a/x.sco"));
        assert_eq!(
            (c.archetype, c.species, c.confidence),
            (Archetype::Pine, Some("pine"), 1.0)
        );
        assert!(c.replace_geometry() && r.is_some());
        let (c, _) = classify_with(&o, &sig("a/y.sco"));
        assert!(!c.replace_geometry());
        assert_eq!(classify_with(&o, &sig("a/z.sco")).0, plain);
    }
}
