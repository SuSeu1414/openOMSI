//! Generalised-cylinder meshing of a skeleton: one tube per stem, bark UVs of constant density.

use std::f32::consts::TAU;

use glam::{Vec2, Vec3};

use super::mesh::Mesh;
use super::skeleton::Skeleton;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TubeOptions {
    /// Wanted ring edge length in m.
    pub edge: f32,
    pub radial_min: u32,
    pub radial_max: u32,
    /// Stems with `radii[0]` below this are skipped (0 keeps all).
    pub min_radius: f32,
    /// Stems with a higher level are skipped.
    pub max_level: u8,
    /// Metres of bark one texture repeat covers around the stem.
    pub bark_tile: f32,
}

impl Default for TubeOptions {
    fn default() -> Self {
        Self {
            edge: 0.04,
            radial_min: 3,
            radial_max: 16,
            min_radius: 0.0,
            max_level: 3,
            bark_tile: 0.5,
        }
    }
}

/// Radial segment count and bark wraps of a stem with base radius `r0`.
fn ring_counts(r0: f32, o: &TubeOptions) -> (usize, f32) {
    let c0 = TAU * r0;
    let n =
        ((c0 / o.edge).round() as u32).clamp(o.radial_min, o.radial_max.max(o.radial_min)) as usize;
    let wraps = (c0 / o.bark_tile).round().max(1.0);
    (n, wraps)
}

pub fn tubes(sk: &Skeleton, o: &TubeOptions) -> Mesh {
    let mut mesh = Mesh::default();
    for s in &sk.stems {
        if s.radii[0] < o.min_radius || s.level > o.max_level || s.points.len() < 2 {
            continue;
        }
        let m = s.points.len();
        let r0 = s.radii[0];
        let (n, wraps) = ring_counts(r0, o);
        let ds: Vec<f32> = s.points.windows(2).map(|w| w[0].distance(w[1])).collect();
        let base = mesh.positions.len() as u32;
        let row = n as u32 + 1;
        let mut v = 0.0f32;
        for i in 0..m {
            let f = if i == 0 || i == m - 1 {
                s.frames[i]
            } else {
                s.frames[i - 1].slerp(s.frames[i], 0.5)
            };
            let h = f * Vec3::Z;
            let (lo, hi) = (i.max(1) - 1, (i + 1).min(m - 1));
            let span: f32 = ds[lo..hi].iter().sum();
            let dr = if span > 0.0 {
                (s.radii[hi] - s.radii[lo]) / span
            } else {
                0.0
            };
            for k in 0..=n {
                let kk = if k == n { 0 } else { k };
                let th = TAU * kk as f32 / n as f32;
                let d = f * Vec3::new(th.cos(), th.sin(), 0.0);
                mesh.positions.push(s.points[i] + d * s.radii[i]);
                mesh.normals.push((d - h * dr).normalize());
                mesh.uvs.push(Vec2::new(wraps * k as f32 / n as f32, v));
            }
            if i + 1 < m {
                let c = (0.5 * (s.radii[i] + s.radii[i + 1])).max(0.25 * r0);
                v += ds[i] * wraps / (TAU * c);
            }
        }
        for i in 0..m as u32 - 1 {
            for k in 0..n as u32 {
                let a = base + i * row + k;
                let (b, c) = (a + 1, a + row);
                mesh.indices.extend_from_slice(&[a, b, c + 1, a, c + 1, c]);
            }
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::Archetype;
    use crate::generator::Stem;
    use crate::generator::{skeleton, Target, TreeParams};
    use crate::presets::preset;
    use glam::Quat;

    fn both() -> [(TreeParams, Target); 2] {
        let t = |height, width| Target { height, width };
        [
            (preset(Archetype::BroadleafWide).unwrap(), t(15.0, 10.0)),
            (preset(Archetype::ConiferConical).unwrap(), t(20.0, 6.0)),
        ]
    }

    #[test]
    fn same_skeleton_same_mesh() {
        let o = TubeOptions::default();
        for (p, t) in both() {
            let d = |seed| tubes(&skeleton(&p, t, seed), &o).digest();
            assert_eq!(d(42), d(42));
            assert_ne!(d(42), d(43));
        }
    }

    #[test]
    fn mesh_is_well_formed() {
        for (p, t) in both() {
            let m = tubes(&skeleton(&p, t, 42), &TubeOptions::default());
            assert_eq!(m.positions.len(), m.normals.len());
            assert_eq!(m.positions.len(), m.uvs.len());
            assert_eq!(m.indices.len() % 3, 0);
            assert!(m.indices.iter().all(|&i| (i as usize) < m.positions.len()));
            assert!(m.normals.iter().all(|n| (n.length() - 1.0).abs() < 1e-4));
            assert!(m.positions.iter().all(|v| v.is_finite()));
            assert!(m.uvs.iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn rings_sit_on_the_radius() {
        let o = TubeOptions::default();
        for (p, t) in both() {
            let sk = skeleton(&p, t, 42);
            let tr = &sk.stems[0];
            let m = tubes(&sk, &o);
            let (n, _) = ring_counts(tr.radii[0], &o);
            for (j, v) in m.positions[..tr.points.len() * (n + 1)].iter().enumerate() {
                let i = j / (n + 1);
                assert!((v.distance(tr.points[i]) - tr.radii[i]).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn faces_point_outwards() {
        let o = TubeOptions::default();
        for (p, t) in both() {
            let sk = skeleton(&p, t, 42);
            let tr = &sk.stems[0];
            let m = tubes(&sk, &o);
            let (n, _) = ring_counts(tr.radii[0], &o);
            let tris = (tr.points.len() - 1) * n * 2;
            for tri in m.indices[..tris * 3].chunks(3) {
                let q: Vec<Vec3> = tri.iter().map(|&i| m.positions[i as usize]).collect();
                let ring = tri[0] as usize / (n + 1);
                let g = (q[1] - q[0]).cross(q[2] - q[0]);
                let out = (q[0] + q[1] + q[2]) / 3.0 - tr.points[ring];
                assert!(g.dot(out) > 0.0);
            }
        }
    }

    #[test]
    fn bark_aspect_is_one_to_one() {
        let sk = Skeleton {
            stems: vec![Stem {
                level: 0,
                parent: None,
                parent_t: 0.0,
                dead: false,
                points: (0..5).map(|i| Vec3::Z * (0.5 * i as f32)).collect(),
                radii: vec![0.3; 5],
                frames: vec![Quat::IDENTITY; 5],
            }],
        };
        let m = tubes(&sk, &TubeOptions::default());
        let (n, wraps) = ring_counts(0.3, &TubeOptions::default());
        assert_eq!(wraps, 4.0);
        let per_m = wraps / (TAU * 0.3);
        let row = n + 1;
        let rel = |a: f32, b: f32| (a / b - 1.0).abs() < 1e-4;
        assert!(rel((m.uvs[row].y - m.uvs[0].y) / 0.5, per_m));
        assert!(rel(m.uvs[n].x / (TAU * 0.3), per_m));
        assert!(rel(m.uvs[4 * row].y, 2.0 * per_m));
    }

    #[test]
    fn v_grows_along_each_stem() {
        let o = TubeOptions::default();
        for (p, t) in both() {
            let sk = skeleton(&p, t, 42);
            let m = tubes(&sk, &o);
            let mut at = 0;
            for s in &sk.stems {
                let (n, wraps) = ring_counts(s.radii[0], &o);
                let row = n + 1;
                for i in 0..s.points.len() {
                    let (a, z) = (at + i * row, at + i * row + n);
                    assert_eq!(m.positions[a], m.positions[z]);
                    assert_eq!(m.normals[a], m.normals[z]);
                    assert_eq!(m.uvs[z].x, wraps);
                    assert_eq!(wraps.fract(), 0.0);
                    if i > 0 {
                        assert!(m.uvs[a].y >= m.uvs[a - row].y);
                    }
                }
                at += s.points.len() * row;
            }
            assert_eq!(at, m.positions.len());
        }
    }

    #[test]
    fn budget() {
        let small = TubeOptions {
            min_radius: 0.01,
            ..Default::default()
        };
        for (p, t) in both() {
            let sk = skeleton(&p, t, 42);
            let (a, b) = (
                tubes(&sk, &TubeOptions::default()).triangles(),
                tubes(&sk, &small).triangles(),
            );
            eprintln!("budget: default {a}, min_radius 0.01 {b}");
            assert!(a <= 25_000);
            assert!(b < a);
        }
    }
}
