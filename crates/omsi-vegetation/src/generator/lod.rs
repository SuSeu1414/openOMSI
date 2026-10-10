//! Levels of detail of a tree: one entry point builds bark and leaf cards for LOD0 (full),
//! LOD1 (one card per twig, simpler bark) or LOD2 (trunk, thick branches, crossed cards).

use glam::Vec3;

use super::bake::{stem_bake, Crown};
use super::leaves::{card_normal, leaves, push_card};
use super::mesh::Mesh;
use super::params::{LeafParams, TreeParams};
use super::skeleton::{sample, skeleton, stem_length, Skeleton, Stem, Target};
use super::tube::{tubes, TubeOptions};
use crate::seed::unit;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lod {
    L0,
    L1,
    L2,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TreeMesh {
    pub bark: Mesh,
    pub leaves: Mesh,
}

impl TreeMesh {
    pub fn triangles(&self) -> usize {
        self.bark.triangles() + self.leaves.triangles()
    }
}

pub fn generate(p: &TreeParams, t: Target, seed: u64, lod: Lod) -> TreeMesh {
    let sk = skeleton(p, t, seed);
    let leaves = match lod {
        Lod::L0 => leaves(&sk, &p.leaves, seed),
        Lod::L1 => twig_cards(&sk, &p.leaves, seed),
        Lod::L2 => branch_cards(&sk, &p.leaves, seed),
    };
    TreeMesh {
        bark: tubes(&sk, &bark_options(lod, &sk), seed),
        leaves,
    }
}

pub fn bark_options(lod: Lod, sk: &Skeleton) -> TubeOptions {
    let outer = sk.stems.iter().map(|s| s.level).max().unwrap_or(0);
    let base = TubeOptions::default();
    match lod {
        Lod::L0 => TubeOptions {
            min_radius: 0.006,
            ..base
        },
        Lod::L1 => TubeOptions {
            edge: 0.08,
            radial_max: 8,
            min_radius: 0.01,
            max_level: outer.saturating_sub(1),
            collars: false,
            ..base
        },
        Lod::L2 => TubeOptions {
            edge: 0.20,
            radial_min: 4,
            radial_max: 6,
            min_radius: 0.3 * sk.stems.first().map_or(0.0, |s| s.radii[0]),
            max_level: 1,
            flare: false,
            collars: false,
            ..base
        },
    }
}

/// `s.frames[0] * X` made perpendicular to the unit `d`.
fn side_of(s: &Stem, d: Vec3) -> Vec3 {
    let v = s.frames[0] * Vec3::X;
    let v = v - d * v.dot(d);
    if v.length() < 1e-3 {
        d.any_orthonormal_vector()
    } else {
        v.normalize()
    }
}

/// LOD1: one card per outermost twig, a bigger terminal card on every other living branch.
fn twig_cards(sk: &Skeleton, lp: &LeafParams, seed: u64) -> Mesh {
    let mut mesh = Mesh::default();
    let (crown, sb) = (Crown::of(sk), stem_bake(sk, seed));
    let outer = sk.stems.iter().map(|s| s.level).max().unwrap_or(0);
    for (idx, s) in sk.stems.iter().enumerate() {
        if s.dead || s.level == 0 {
            continue;
        }
        let rand = unit(seed ^ 0xCA4D_0003, idx as u64);
        if s.level == outer {
            let p0 = sample(s, lp.start).0;
            let tip = *s.points.last().unwrap();
            let d = (tip - p0)
                .try_normalize()
                .unwrap_or(*s.frames.last().unwrap() * Vec3::Z);
            let len = p0.distance(tip) + lp.size;
            let n0 = ((1.0 - lp.start) * stem_length(s) / (lp.spacing * lp.size)).floor() + 1.0;
            let width = (n0 * lp.size * lp.size / len).clamp(lp.size, 3.0 * lp.size);
            let n = card_normal(0.5 * (p0 + tip), d, side_of(s, d), lp.up);
            push_card(&mut mesh, p0, d, n, width, len, &crown, &sb[idx], rand);
        } else {
            let (pos, q, _) = sample(s, 1.0);
            let (d, side) = (q * Vec3::Z, q * Vec3::X);
            let n = card_normal(pos, d, side, lp.up);
            let big = 1.5 * lp.size;
            push_card(&mut mesh, pos, d, n, big, big, &crown, &sb[idx], rand);
        }
    }
    mesh
}

/// Unit axis of the crossed cards of main branch `s`.
fn branch_axis(s: &Stem) -> Vec3 {
    (*s.points.last().unwrap() - s.points[0])
        .try_normalize()
        .unwrap_or(Vec3::Z)
}

/// LOD2: two crossed cards over the foliage of each living main branch and its descendants.
fn branch_cards(sk: &Skeleton, lp: &LeafParams, seed: u64) -> Mesh {
    let mut mesh = Mesh::default();
    let (crown, sb) = (Crown::of(sk), stem_bake(sk, seed));
    let outer = sk.stems.iter().map(|s| s.level).max().unwrap_or(0);
    let mut owner: Vec<Option<usize>> = Vec::with_capacity(sk.stems.len());
    for (i, s) in sk.stems.iter().enumerate() {
        owner.push(match (s.level, s.parent) {
            (0, _) => None,
            (1, _) => Some(i),
            (_, p) => p.and_then(|p| owner[p as usize]),
        });
    }
    // Per main branch: the foliage points (outermost stems from `lp.start` on, the tips of the
    // others) as (along the axis from the branch base, distance off the axis).
    let mut ext: Vec<Vec<(f32, f32)>> = vec![Vec::new(); sk.stems.len()];
    for (s, o) in sk.stems.iter().zip(&owner) {
        let Some(o) = *o else { continue };
        let main = &sk.stems[o];
        if s.dead || main.dead {
            continue;
        }
        let (o0, d) = (main.points[0], branch_axis(main));
        let last = s.points.len() - 1;
        let mut pts = Vec::new();
        if s.level == outer {
            pts.push(sample(s, lp.start).0);
            pts.extend(
                (0..=last)
                    .filter_map(|k| (k as f32 / last as f32 >= lp.start).then_some(s.points[k])),
            );
        } else {
            pts.push(s.points[last]);
        }
        for q in pts {
            let r = q - o0;
            let a = r.dot(d);
            ext[o].push((a, (r - d * a).length()));
        }
    }
    for (idx, s) in sk.stems.iter().enumerate() {
        if s.dead || s.level != 1 {
            continue;
        }
        let (o0, d) = (s.points[0], branch_axis(s));
        let lo = ext[idx].iter().fold(f32::MAX, |m, e| m.min(e.0));
        let hi = ext[idx].iter().fold(f32::MIN, |m, e| m.max(e.0));
        let perp = ext[idx].iter().fold(0.0f32, |m, e| m.max(e.1));
        let (lo, hi) = if lo > hi { (0.0, 0.0) } else { (lo, hi) };
        let p0 = o0 + d * lo;
        let len = (hi - lo).max(lp.size) + lp.size;
        let width = (2.0 * perp + lp.size).min(1.5 * len);
        let n1 = card_normal(p0 + d * 0.5 * len, d, side_of(s, d), lp.up);
        let n2 = d.cross(n1);
        let rand = unit(seed ^ 0xCA4D_0004, idx as u64);
        for n in [n1, n2] {
            push_card(&mut mesh, p0, d, n, width, len, &crown, &sb[idx], rand);
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::Archetype;
    use crate::presets::{preset, TARGETS};

    const LODS: [Lod; 3] = [Lod::L0, Lod::L1, Lod::L2];

    fn tree(a: Archetype, (height, width): (f32, f32), seed: u64, lod: Lod) -> TreeMesh {
        generate(&preset(a).unwrap(), Target { height, width }, seed, lod)
    }

    fn digest(m: &TreeMesh) -> (u64, u64) {
        (m.bark.digest(), m.leaves.digest())
    }

    #[test]
    fn lod_budgets() {
        let mut over = Vec::new();
        for (a, h, w) in TARGETS {
            let t = LODS.map(|l| tree(a, (h, w), 42, l));
            let row = t.each_ref().map(|m| {
                let (b, l) = (m.bark.triangles(), m.leaves.triangles());
                format!("{b}/{l}/{}", b + l)
            });
            println!("{a:?}: L0 {} | L1 {} | L2 {}", row[0], row[1], row[2]);
            let n = t.each_ref().map(TreeMesh::triangles);
            if n[0] > 30_000 || n[1] > 8_000 || n[2] > 2_000 || !(n[0] > n[1] && n[1] > n[2]) {
                over.push(format!("{a:?} {n:?}"));
            }
        }
        assert!(over.is_empty(), "{over:?}");
    }

    #[test]
    fn same_seed_same_lods() {
        for lod in LODS {
            for (a, h, w) in [TARGETS[0], TARGETS[1]] {
                let d = |seed| digest(&tree(a, (h, w), seed, lod));
                assert_eq!(d(42), d(42), "{a:?} {lod:?}");
                assert_ne!(d(42), d(43), "{a:?} {lod:?}");
            }
        }
    }

    #[test]
    fn lod0_is_the_full_leaves() {
        for (a, height, width) in TARGETS {
            let p = preset(a).unwrap();
            let t = Target { height, width };
            let m = generate(&p, t, 42, Lod::L0);
            assert_eq!(
                m.leaves,
                leaves(&skeleton(&p, t, 42), &p.leaves, 42),
                "{a:?}"
            );
        }
    }

    fn area(m: &Mesh) -> f32 {
        m.indices
            .chunks(3)
            .map(|t| {
                let q = [0, 1, 2].map(|j| m.positions[t[j] as usize]);
                0.5 * (q[1] - q[0]).cross(q[2] - q[0]).length()
            })
            .sum()
    }

    #[test]
    fn lods_keep_the_coverage() {
        let mut bad = Vec::new();
        for (a, h, w) in TARGETS {
            let [a0, a1, a2] = LODS.map(|l| area(&tree(a, (h, w), 42, l).leaves));
            println!("{a:?}: L1/L0 {:.2}, L2/L0 {:.2}", a1 / a0, a2 / a0);
            // L2 bound is wider: a crossed pair counts its area twice.
            if !(0.5..=2.0).contains(&(a1 / a0)) || !(0.3..=3.0).contains(&(a2 / a0)) {
                bad.push(a);
            }
        }
        assert!(bad.is_empty(), "{bad:?}");
    }

    #[test]
    fn lods_stay_in_the_crown() {
        for lod in LODS {
            for (a, h, w) in TARGETS {
                let p = preset(a).unwrap();
                let size = p.leaves.size;
                for seed in 0..4 {
                    let m = tree(a, (h, w), seed, lod).leaves;
                    let reach = w / 2.0 + w * p.asymmetry / 2.0 + 2.0 * size;
                    for v in &m.positions {
                        assert!(v.z <= h + 2.0 * size, "{a:?} {lod:?} {seed}");
                        assert!(v.truncate().length() <= reach, "{a:?} {lod:?} {seed}");
                    }
                }
            }
        }
    }

    #[test]
    fn lod_meshes_are_well_formed() {
        for lod in LODS {
            for (a, h, w) in TARGETS {
                let t = tree(a, (h, w), 42, lod);
                for m in [&t.bark, &t.leaves] {
                    let n = m.positions.len();
                    assert!([m.normals.len(), m.uvs.len(), m.attrs.len()] == [n; 3]);
                    assert!(m.indices.iter().all(|&i| (i as usize) < n));
                    assert!(m.positions.iter().all(|v| v.is_finite()));
                    assert!(m.uvs.iter().all(|v| v.is_finite()));
                    assert!(m.normals.iter().all(|v| (v.length() - 1.0).abs() < 1e-4));
                }
                assert_eq!(t.leaves.positions.len() % 8, 0, "{a:?} {lod:?}");
            }
        }
    }
}
