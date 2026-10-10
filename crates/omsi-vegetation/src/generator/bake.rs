//! Per-vertex data for the wind and leaf shaders: wind pivots, phase, stiffness, crown AO and
//! crown normal, all derived from the skeleton and the seed.

use glam::{Vec3, Vec4};

use super::mesh::VertexAttrs;
use super::skeleton::{stem_length, Skeleton};
use crate::seed::unit;

/// The crown as an ellipsoid fitted to the box of the branches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crown {
    pub center: Vec3,
    pub radii: Vec3,
}

impl Crown {
    /// Axis-aligned box of all points of stems with level >= 1 (trunk points if there are none);
    /// center = box centre, radii = half extents, each at least 0.25.
    pub fn of(sk: &Skeleton) -> Crown {
        let pts = |min_level: u8| -> Vec<Vec3> {
            sk.stems
                .iter()
                .filter(|s| s.level >= min_level)
                .flat_map(|s| s.points.iter().copied())
                .collect()
        };
        let mut src = pts(1);
        if src.is_empty() {
            src = pts(0);
        }
        if src.is_empty() {
            return Crown {
                center: Vec3::ZERO,
                radii: Vec3::splat(0.25),
            };
        }
        let (lo, hi) = src.iter().fold(
            (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
            |(l, h), &p| (l.min(p), h.max(p)),
        );
        Crown {
            center: 0.5 * (lo + hi),
            radii: (0.5 * (hi - lo)).max(Vec3::splat(0.25)),
        }
    }

    /// Unit normal of the ellipsoid at `p` (gradient of the implicit form).
    pub fn normal(&self, p: Vec3) -> Vec3 {
        let g = (p - self.center) / (self.radii * self.radii);
        if g.length() < 1e-6 {
            Vec3::Z
        } else {
            g.normalize()
        }
    }

    /// Crown-depth occlusion in [0, 1]: dark in the middle, open at the rim, a bit darker low.
    pub fn ao(&self, p: Vec3) -> f32 {
        let d = ((p - self.center) / self.radii).length().min(1.0);
        let hz = ((p.z - (self.center.z - self.radii.z)) / (2.0 * self.radii.z)).clamp(0.0, 1.0);
        (0.3 + 0.7 * d.powf(1.5)) * (0.85 + 0.15 * hz)
    }
}

/// What every vertex of one stem shares.
pub(crate) struct StemBake {
    pub branch: Vec4,
    pub twig: Vec4,
    pub phase: f32,
    pub stiffness: f32,
    pub rand: f32,
}

/// One entry per stem, same index (parents always have a lower index).
pub(crate) fn stem_bake(sk: &Skeleton, seed: u64) -> Vec<StemBake> {
    let trunk_r = sk.stems.first().map_or(1.0, |s| s.radii[0]);
    let mut out: Vec<StemBake> = Vec::with_capacity(sk.stems.len());
    for (i, s) in sk.stems.iter().enumerate() {
        let pivot = s
            .points
            .first()
            .map_or(Vec4::ZERO, |p| p.extend(stem_length(s)));
        let up = s.parent.map(|p| &out[p as usize]);
        let (branch, twig, phase) = match (s.level, up) {
            (0, _) | (_, None) => (Vec4::ZERO, Vec4::ZERO, 0.0),
            (1, _) => (pivot, pivot, unit(seed ^ 0xA5E0_F00D, i as u64)),
            (2, Some(u)) => (u.branch, pivot, u.phase),
            (_, Some(u)) => (u.branch, u.twig, u.phase),
        };
        out.push(StemBake {
            branch,
            twig,
            phase,
            stiffness: (s.radii[0] / trunk_r.max(1e-6)).sqrt().clamp(0.0, 1.0),
            rand: unit(seed ^ 0x7A4D_0001, i as u64),
        });
    }
    out
}

pub(crate) fn attrs(
    c: &Crown,
    s: &StemBake,
    p: Vec3,
    flutter: f32,
    leaf: bool,
    rand: f32,
) -> VertexAttrs {
    VertexAttrs {
        branch: s.branch,
        twig: s.twig,
        crown_normal: c.normal(p),
        flutter,
        ao: c.ao(p),
        phase: s.phase,
        stiffness: if leaf { 0.0 } else { s.stiffness },
        rand,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::Archetype;
    use crate::generator::{leaves, skeleton, tubes, Mesh, Target, TubeOptions, HULL};
    use crate::presets::{preset, TARGETS};

    fn bw() -> (Skeleton, Mesh, Mesh) {
        let p = preset(Archetype::BroadleafWide).unwrap();
        let t = Target {
            height: 15.0,
            width: 10.0,
        };
        let sk = skeleton(&p, t, 42);
        let (tu, le) = (
            tubes(&sk, &TubeOptions::default(), 42),
            leaves(&sk, &p.leaves, 42),
        );
        (sk, tu, le)
    }

    #[test]
    fn attrs_cover_every_vertex() {
        for (a, height, width) in TARGETS {
            let p = preset(a).unwrap();
            let sk = skeleton(&p, Target { height, width }, 42);
            for m in [
                tubes(&sk, &TubeOptions::default(), 42),
                leaves(&sk, &p.leaves, 42),
            ] {
                assert_eq!(m.attrs.len(), m.positions.len(), "{a:?}");
                for v in &m.attrs {
                    let all = v
                        .branch
                        .to_array()
                        .into_iter()
                        .chain(v.twig.to_array())
                        .chain(v.crown_normal.to_array())
                        .chain([v.flutter, v.ao, v.phase, v.stiffness, v.rand]);
                    assert!(all.into_iter().all(f32::is_finite));
                    for x in [v.ao, v.phase, v.stiffness, v.rand, v.flutter] {
                        assert!((0.0..=1.0).contains(&x), "{a:?} {x}");
                    }
                    assert!((v.crown_normal.length() - 1.0).abs() < 1e-4);
                }
            }
        }
    }

    #[test]
    fn pivots_follow_the_hierarchy() {
        let (sk, _, _) = bw();
        let sb = stem_bake(&sk, 42);
        assert_eq!(sb[0].branch, Vec4::ZERO);
        assert_eq!(sb[0].stiffness, 1.0);
        let (mut l1, mut l2) = (0, 0);
        for (i, s) in sk.stems.iter().enumerate() {
            let own = s.points[0].extend(stem_length(s));
            match s.level {
                1 => {
                    l1 += 1;
                    assert_eq!(sb[i].branch, own);
                }
                2 => {
                    l2 += 1;
                    let u = &sb[s.parent.unwrap() as usize];
                    assert_eq!(sb[i].branch, u.branch);
                    assert_eq!(sb[i].phase, u.phase);
                    assert_eq!(sb[i].twig, own);
                }
                _ => {}
            }
        }
        assert!(l1 > 0 && l2 > 0);
    }

    fn geometry(m: &Mesh) -> u64 {
        Mesh {
            attrs: Vec::new(),
            ..m.clone()
        }
        .digest()
    }

    #[test]
    fn bark_keeps_its_geometry() {
        // Recorded with the code of task 012, before the attrs existed.
        let want = [
            (
                Archetype::BroadleafWide,
                (15.0, 10.0),
                0xbc7246b60efc0747,
                0xe33098cf3280a8e5,
            ),
            (
                Archetype::ConiferConical,
                (20.0, 6.0),
                0xe4ae7103c08e81bb,
                0x12b13b3ededfe98b,
            ),
        ];
        for (a, (height, width), tube, leaf) in want {
            let p = preset(a).unwrap();
            let sk = skeleton(&p, Target { height, width }, 42);
            let t = tubes(&sk, &TubeOptions::default(), 42);
            assert_eq!(geometry(&t), tube, "{a:?}");
            assert_eq!(geometry(&leaves(&sk, &p.leaves, 42)), leaf, "{a:?}");
        }
    }

    #[test]
    fn leaf_flutter_is_hull_v() {
        let (_, _, l) = bw();
        for (k, v) in l.attrs.iter().enumerate() {
            assert_eq!(v.flutter, HULL[k % 8].y);
            assert_eq!(v.stiffness, 0.0);
        }
    }

    #[test]
    fn crown_normal_points_out() {
        for (a, height, width, min_mean) in [
            (Archetype::BroadleafWide, 15.0, 10.0, 0.8),
            (Archetype::ConiferConical, 20.0, 6.0, 0.6),
        ] {
            let p = preset(a).unwrap();
            let sk = skeleton(&p, Target { height, width }, 42);
            let (c, l) = (Crown::of(&sk), leaves(&sk, &p.leaves, 42));
            for (&q, v) in l.positions.iter().zip(&l.attrs) {
                assert!(v.crown_normal.dot(q - c.center) > 0.0, "{a:?}");
            }
            let sum: f32 = l
                .positions
                .iter()
                .zip(&l.attrs)
                .map(|(&p, v)| v.crown_normal.dot((p - c.center).normalize_or_zero()))
                .sum();
            let mean = sum / l.positions.len() as f32;
            assert!(mean > min_mean, "{a:?} {mean}");
        }
    }

    #[test]
    fn ao_darkens_the_inside() {
        let (sk, _, l) = bw();
        let c = Crown::of(&sk);
        let mean = |f: &dyn Fn(f32) -> bool| {
            let v: Vec<f32> = l
                .positions
                .iter()
                .zip(&l.attrs)
                .filter(|(&p, _)| f(((p - c.center) / c.radii).length()))
                .map(|(_, a)| a.ao)
                .collect();
            assert!(!v.is_empty());
            v.iter().sum::<f32>() / v.len() as f32
        };
        let (inner, outer) = (mean(&|d| d < 0.6), mean(&|d| d > 0.9));
        assert!(outer - inner >= 0.15, "{inner} {outer}");
        assert!(c.ao(c.center) < 0.35);
        assert!((c.ao(c.center + Vec3::Z * c.radii.z) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn cards_get_their_own_random() {
        let (_, _, l) = bw();
        let cards = l.positions.len() / 8;
        let mut r: Vec<u32> = l.attrs.iter().map(|a| a.rand.to_bits()).collect();
        r.sort_unstable();
        r.dedup();
        assert!(
            r.len() as f32 >= 0.9 * cards as f32,
            "{} of {cards}",
            r.len()
        );
    }
}
