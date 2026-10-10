//! Leaf cards: twig-cluster quads cropped to a fixed 8-vertex convex hull, placed along the
//! outermost stems. Pure data; textures come later and are drawn inside `HULL`.

use glam::{Quat, Vec2, Vec3};

use super::bake::{attrs, stem_bake, Crown};
use super::mesh::Mesh;
use super::params::LeafParams;
use super::skeleton::{sample, Skeleton, Stem};
use crate::seed::{splitmix64, unit, Rng};

/// The card outline in card space (u across, v from base 0 to tip 1), counter-clockwise, convex.
pub const HULL: [Vec2; 8] = [
    Vec2::new(0.42, 0.00),
    Vec2::new(0.58, 0.00),
    Vec2::new(0.88, 0.22),
    Vec2::new(1.00, 0.58),
    Vec2::new(0.80, 0.96),
    Vec2::new(0.20, 0.96),
    Vec2::new(0.00, 0.58),
    Vec2::new(0.12, 0.22),
];
pub const CARD_TRIANGLES: usize = 6;

pub(super) fn length(s: &Stem) -> f32 {
    s.points.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Leaf cards of a skeleton: 8 vertices and 6 triangles per card, in card order.
pub fn leaves(sk: &Skeleton, lp: &LeafParams, seed: u64) -> Mesh {
    let mut mesh = Mesh::default();
    let crown = Crown::of(sk);
    let sb = stem_bake(sk, seed);
    let mut card = 0u64;
    let outer = sk.stems.iter().map(|s| s.level).max().unwrap_or(0);
    for (idx, s) in sk.stems.iter().enumerate() {
        if s.dead || s.level == 0 {
            continue;
        }
        let mut r = Rng::new(splitmix64(
            seed ^ 0x1EAF_CA4D ^ (idx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15),
        ));
        // (t, angle in degrees, roll in degrees)
        let mut cards = Vec::new();
        if s.level == outer {
            let n = ((1.0 - lp.start) * length(s) / (lp.spacing * lp.size)) as usize;
            let roll0 = r.unit() * 360.0;
            for j in 0..n {
                let t = lp.start + (1.0 - lp.start) * (j as f32 + 0.5) / n as f32;
                let ang = lp.angle + r.signed() * lp.angle_v;
                cards.push((t, ang, roll0 + 180.0 * j as f32 + r.signed() * 30.0));
            }
        }
        cards.push((1.0, 0.0, 0.0));
        for (t, ang, roll) in cards {
            let size = lp.size * (1.0 + 0.2 * r.signed());
            let (pos, q, _) = sample(s, t);
            let h = q * Vec3::Z;
            let (sr, cr) = roll.to_radians().sin_cos();
            let a = q * Vec3::new(cr, sr, 0.0);
            let (sa, ca) = ang.to_radians().sin_cos();
            let d = (h * ca + a * sa).normalize();
            let flat = Vec3::new(pos.x, pos.y, 0.0);
            let out = if flat.length() < 1e-3 {
                a
            } else {
                flat.normalize()
            };
            let f = out.lerp(Vec3::Z, lp.up).normalize();
            let n = f - d * f.dot(d);
            let n = if n.length() < 1e-3 {
                a.cross(d).normalize()
            } else {
                n.normalize()
            };
            let n = Quat::from_axis_angle(d, (r.signed() * 25.0).to_radians()) * n;
            let w = d.cross(n);
            let base = mesh.positions.len() as u32;
            let card_rand = unit(seed ^ 0xCA4D_0002, card);
            card += 1;
            for uv in HULL {
                let p = pos + w * (uv.x - 0.5) * size + d * uv.y * size;
                mesh.positions.push(p);
                mesh.attrs
                    .push(attrs(&crown, &sb[idx], p, uv.y, true, card_rand));
                mesh.normals.push(n);
                mesh.uvs.push(uv);
            }
            for i in 1..=CARD_TRIANGLES as u32 {
                mesh.indices.extend([base, base + i, base + i + 1]);
            }
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::{skeleton, Target};
    use crate::presets::{preset, TARGETS};

    fn each(seed: u64, mut f: impl FnMut(&Skeleton, &LeafParams, Target, Mesh)) {
        for (a, height, width) in TARGETS {
            let p = preset(a).unwrap();
            let t = Target { height, width };
            let sk = skeleton(&p, t, seed);
            f(&sk, &p.leaves, t, leaves(&sk, &p.leaves, seed));
        }
    }

    #[test]
    fn same_seed_same_leaves() {
        each(42, |sk, lp, _, m| {
            assert_eq!(m.digest(), leaves(sk, lp, 42).digest());
            assert_ne!(m.digest(), leaves(sk, lp, 43).digest());
        });
    }

    #[test]
    fn cards_are_well_formed() {
        each(7, |_, _, _, m| {
            let cards = m.positions.len() / 8;
            assert_eq!(m.positions.len(), 8 * cards);
            assert_eq!(m.indices.len(), 18 * cards);
            assert!(m.indices.iter().all(|&i| (i as usize) < m.positions.len()));
            assert!(m.positions.iter().all(|p| p.is_finite()));
            for c in 0..cards {
                let p = &m.positions[8 * c..8 * c + 8];
                let n = m.normals[8 * c];
                assert!(m.normals[8 * c..8 * c + 8]
                    .iter()
                    .all(|v| (v.length() - 1.0).abs() < 1e-4));
                for q in p {
                    assert!((*q - p[0]).dot(n).abs() < 1e-4);
                }
                for i in 1..7 {
                    assert!((p[i] - p[0]).cross(p[i + 1] - p[0]).dot(n) > 0.0);
                }
            }
        });
    }

    #[test]
    fn cards_sit_on_twigs() {
        let dist = |p: Vec3, a: Vec3, b: Vec3| {
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
            p.distance(a + ab * t)
        };
        each(7, |sk, _, _, m| {
            for c in m.positions.chunks(8) {
                let mid = 0.5 * (c[0] + c[1]);
                assert!(sk
                    .stems
                    .iter()
                    .filter(|s| !s.dead && s.level >= 1)
                    .any(|s| s.points.windows(2).any(|w| dist(mid, w[0], w[1]) < 0.01)));
            }
        });
    }

    #[test]
    fn cards_stay_near_the_crown() {
        for seed in 0..4 {
            for (a, height, width) in TARGETS {
                let p = preset(a).unwrap();
                let sk = skeleton(&p, Target { height, width }, seed);
                let (lp, m) = (p.leaves, leaves(&sk, &p.leaves, seed));
                let reach = width / 2.0 + width * p.asymmetry / 2.0 + 1.2 * lp.size;
                for v in &m.positions {
                    assert!(v.z <= height + 1.2 * lp.size, "{a:?} {seed}");
                    assert!(v.truncate().length() <= reach, "{a:?} {seed}");
                }
            }
        }
    }

    #[test]
    fn broadleaf_faces_up() {
        let p = preset(crate::classify::Archetype::BroadleafWide).unwrap();
        let sk = skeleton(
            &p,
            Target {
                height: 15.0,
                width: 10.0,
            },
            42,
        );
        let m = leaves(&sk, &p.leaves, 42);
        let mean = m.normals.iter().map(|n| n.z).sum::<f32>() / m.normals.len() as f32;
        assert!(mean > 0.3, "{mean}");
    }

    #[test]
    fn no_cards_on_dead_or_trunk() {
        each(42, |sk, lp, _, _| {
            let mut dead = sk.clone();
            dead.stems.iter_mut().for_each(|s| s.dead = true);
            assert!(leaves(&dead, lp, 42).positions.is_empty());
            let mut trunk = sk.clone();
            trunk.stems.truncate(1);
            assert!(leaves(&trunk, lp, 42).positions.is_empty());
        });
    }

    #[test]
    fn leaf_budget() {
        for (a, height, width) in TARGETS {
            let p = preset(a).unwrap();
            let sk = skeleton(&p, Target { height, width }, 42);
            let m = leaves(&sk, &p.leaves, 42);
            let cards = m.positions.len() / 8;
            println!("{a:?}: cards {cards}, triangles {}", m.triangles());
            assert!((100..=3000).contains(&cards), "{a:?} {cards}");
        }
    }
}
