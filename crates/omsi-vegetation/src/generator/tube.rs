//! Generalised-cylinder meshing of a skeleton: one tube per stem, bark UVs of constant density.

use std::f32::consts::TAU;

use glam::{Vec2, Vec3};

use super::mesh::Mesh;
use super::rings::{collar_radius, flare_radius, resample, Ring};
use super::skeleton::{sample, Flare, Skeleton, Stem};

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

/// How one stem is meshed: its rings, radial count and the radius modifiers.
struct Plan {
    rings: Vec<Ring>,
    n: usize,
    wraps: f32,
    flare: Option<Flare>,
    /// Parent radius where a collar starts.
    collar: Option<f32>,
}

fn plan(sk: &Skeleton, s: &Stem, o: &TubeOptions) -> Plan {
    let r0 = s.radii[0];
    let flare = if s.parent.is_none() { sk.flare } else { None };
    let collar = s
        .parent
        .map(|p| sample(&sk.stems[p as usize], s.parent_t).2)
        .filter(|&rp| r0 >= 0.025 && rp > r0);
    let mut extra = Vec::new();
    if let Some(f) = flare {
        extra.push(-0.5 * r0);
        extra.extend([0.05, 0.12, 0.25, 0.45, 0.7, 1.0].map(|x| f.zone * x));
    }
    if let Some(rp) = collar {
        extra.extend([0.0, 1.5, 3.0].map(|x| rp + x * r0));
    }
    let (mut n, wraps) = ring_counts(r0, o);
    if let Some(f) = flare {
        n = n.max(8 * f.buttresses as usize).min(40);
    }
    Plan {
        rings: resample(s, &extra),
        n,
        wraps,
        flare,
        collar,
    }
}

pub fn tubes(sk: &Skeleton, o: &TubeOptions) -> Mesh {
    let mut mesh = Mesh::default();
    for s in &sk.stems {
        if s.radii[0] < o.min_radius || s.level > o.max_level || s.points.len() < 2 {
            continue;
        }
        let r0 = s.radii[0];
        let Plan {
            rings,
            n,
            wraps,
            flare,
            collar,
        } = plan(sk, s, o);
        let m = rings.len();
        let base = mesh.positions.len() as u32;
        let row = n as u32 + 1;
        let mut v = 0.0f32;
        for (i, ring) in rings.iter().enumerate() {
            let h = ring.frame * Vec3::Z;
            let (lo, hi) = (i.max(1) - 1, (i + 1).min(m - 1));
            let span = rings[hi].s - rings[lo].s;
            let dr = if span > 0.0 {
                (rings[hi].radius - rings[lo].radius) / span
            } else {
                0.0
            };
            for k in 0..=n {
                let kk = if k == n { 0 } else { k };
                let th = TAU * kk as f32 / n as f32;
                let d = ring.frame * Vec3::new(th.cos(), th.sin(), 0.0);
                let radius = match (flare, collar) {
                    (Some(f), _) => flare_radius(&f, ring, th),
                    (None, Some(rp)) => collar_radius(ring, rp, r0),
                    _ => ring.radius,
                };
                mesh.positions.push(ring.pos + d * radius);
                mesh.normals.push((d - h * dr).normalize());
                mesh.uvs.push(Vec2::new(wraps * k as f32 / n as f32, v));
            }
            if i + 1 < m {
                let c = (0.5 * (ring.radius + rings[i + 1].radius)).max(0.25 * r0);
                v += (rings[i + 1].s - ring.s) * wraps / (TAU * c);
            }
        }
        let first = mesh.indices.len();
        for i in 0..m as u32 - 1 {
            for k in 0..n as u32 {
                let a = base + i * row + k;
                let (b, c) = (a + 1, a + row);
                mesh.indices.extend_from_slice(&[a, b, c + 1, a, c + 1, c]);
            }
        }
        if flare.is_some() || collar.is_some() {
            smooth_normals(&mut mesh, base, m, n, first);
        }
    }
    mesh
}

/// Replace the analytic normals of the stem starting at vertex `base` by area-weighted face
/// normals; the seam vertices (k = 0 and k = n) share one sum.
fn smooth_normals(mesh: &mut Mesh, base: u32, rings: usize, n: usize, first: usize) {
    let at = |i: u32| (i - base) as usize;
    let mut acc = vec![Vec3::ZERO; rings * (n + 1)];
    for t in mesh.indices[first..].chunks(3) {
        let q = [0, 1, 2].map(|j| mesh.positions[t[j] as usize]);
        let g = (q[1] - q[0]).cross(q[2] - q[0]);
        t.iter().for_each(|&i| acc[at(i)] += g);
    }
    for j in 0..rings {
        let (a, z) = (j * (n + 1), j * (n + 1) + n);
        let sum = acc[a] + acc[z];
        acc[a] = sum;
        acc[z] = sum;
        for k in 0..=n {
            if acc[a + k].length() >= 1e-12 {
                mesh.normals[base as usize + a + k] = acc[a + k].normalize();
            }
        }
    }
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
            let mut sk = skeleton(&p, t, 42);
            sk.flare = None;
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
            let mut sk = skeleton(&p, t, 42);
            sk.flare = None;
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
            flare: None,
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
                let pl = plan(&sk, s, &o);
                let (n, wraps) = (pl.n, pl.wraps);
                let row = n + 1;
                for i in 0..pl.rings.len() {
                    let (a, z) = (at + i * row, at + i * row + n);
                    assert_eq!(m.positions[a], m.positions[z]);
                    assert_eq!(m.normals[a], m.normals[z]);
                    assert_eq!(m.uvs[z].x, wraps);
                    assert_eq!(wraps.fract(), 0.0);
                    if i > 0 {
                        assert!(m.uvs[a].y >= m.uvs[a - row].y);
                    }
                }
                at += pl.rings.len() * row;
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

    const BW: Target = Target {
        height: 15.0,
        width: 10.0,
    };

    fn broadleaf() -> Skeleton {
        skeleton(&preset(Archetype::BroadleafWide).unwrap(), BW, 42)
    }

    /// Mean distance of the vertices of ring `j` of the stem whose plan is `pl` from its centre.
    fn mean_dist(m: &Mesh, at: usize, pl: &Plan, j: usize) -> f32 {
        let row = pl.n + 1;
        let vs = &m.positions[at + j * row..at + j * row + pl.n];
        vs.iter().map(|v| v.distance(pl.rings[j].pos)).sum::<f32>() / pl.n as f32
    }

    #[test]
    fn flare_widens_the_foot() {
        let (sk, o) = (broadleaf(), TubeOptions::default());
        let m = tubes(&sk, &o);
        let pl = plan(&sk, &sk.stems[0], &o);
        let (f, r0) = (sk.flare.unwrap(), sk.stems[0].radii[0]);
        let j0 = pl.rings.iter().position(|r| r.s == 0.0).unwrap();
        let d0 = mean_dist(&m, 0, &pl, j0) / r0;
        assert!((1.4..=2.0).contains(&d0), "foot {d0}");
        let jz = pl
            .rings
            .iter()
            .position(|r| (r.s - f.zone).abs() < 1e-4)
            .unwrap();
        let d = mean_dist(&m, 0, &pl, jz) / pl.rings[jz].radius;
        assert!((d - 1.0).abs() < 0.02, "zone {d}");
        let low = m.positions[..pl.rings.len() * (pl.n + 1)]
            .iter()
            .map(|v| v.z)
            .fold(f32::MAX, f32::min);
        assert!(low < -0.2 * r0, "lowest {low}");
    }

    #[test]
    fn buttresses_are_lobes() {
        let (sk, o) = (broadleaf(), TubeOptions::default());
        let m = tubes(&sk, &o);
        let pl = plan(&sk, &sk.stems[0], &o);
        let j0 = pl.rings.iter().position(|r| r.s == 0.0).unwrap();
        let d: Vec<f32> = m.positions[j0 * (pl.n + 1)..][..pl.n]
            .iter()
            .map(|v| v.distance(pl.rings[j0].pos))
            .collect();
        let peaks = (0..pl.n)
            .filter(|&k| d[k] > d[(k + pl.n - 1) % pl.n] + 1e-4 && d[k] > d[(k + 1) % pl.n] + 1e-4)
            .count();
        assert_eq!(peaks as u32, sk.flare.unwrap().buttresses);
    }

    #[test]
    fn flare_is_seeded() {
        let [(b, t), (c, tc)] = both();
        assert_eq!(skeleton(&b, t, 5).flare, skeleton(&b, t, 5).flare);
        let kinds: std::collections::BTreeSet<u32> = (0..32)
            .map(|seed| {
                let f = skeleton(&b, t, seed).flare.unwrap();
                assert!((3..=5).contains(&f.buttresses));
                assert!((0.0..TAU).contains(&f.phase));
                f.buttresses
            })
            .collect();
        assert!(kinds.len() >= 2);
        assert!(skeleton(&c, tc, 42).flare.is_some());
    }

    #[test]
    fn collars_blend_into_the_parent() {
        let (sk, o) = (broadleaf(), TubeOptions::default());
        let m = tubes(&sk, &o);
        let mut at = 0;
        for s in &sk.stems {
            let pl = plan(&sk, s, &o);
            if let (1, Some(rp)) = (s.level, pl.collar) {
                let j = pl
                    .rings
                    .iter()
                    .position(|r| (r.s - rp).abs() < 1e-4)
                    .unwrap();
                let want = (1.8 * pl.rings[j].radius).min(0.95 * rp);
                assert!((mean_dist(&m, at, &pl, j) / want - 1.0).abs() < 0.02);
                let r0 = s.radii[0];
                let end = pl
                    .rings
                    .iter()
                    .position(|r| (r.s - rp - 3.0 * r0).abs() < 1e-4);
                if let Some(j) = end {
                    let d = mean_dist(&m, at, &pl, j) / pl.rings[j].radius;
                    assert!((d - 1.0).abs() < 0.02, "collar end {d}");
                }
                return;
            }
            at += pl.rings.len() * (pl.n + 1);
        }
        panic!("no collared level-1 stem");
    }

    #[test]
    fn modified_normals_follow_the_surface() {
        let (sk, o) = (broadleaf(), TubeOptions::default());
        let m = tubes(&sk, &o);
        let pl = plan(&sk, &sk.stems[0], &o);
        let f = sk.flare.unwrap();
        let j0 = pl.rings.iter().position(|r| r.s == 0.0).unwrap();
        let (mut lobe, mut changed) = (0, 0);
        for k in 0..pl.n {
            let i = j0 * (pl.n + 1) + k;
            let ring = &pl.rings[j0];
            let th = TAU * k as f32 / pl.n as f32;
            let d = ring.frame * Vec3::new(th.cos(), th.sin(), 0.0);
            let h = ring.frame * Vec3::Z;
            let dr = (pl.rings[j0 + 1].radius - ring.radius) / (pl.rings[j0 + 1].s - ring.s);
            let analytic = (d - h * dr).normalize();
            assert!(m.normals[i].dot((m.positions[i] - ring.pos).normalize()) > 0.0);
            if (f.buttresses as f32 * (th - f.phase))
                .cos()
                .max(0.0)
                .powi(3)
                > 0.1
            {
                lobe += 1;
                changed += ((m.normals[i] - analytic).length() > 1e-3) as usize;
            }
        }
        assert!(lobe > 0 && 2 * changed >= lobe, "{changed}/{lobe}");
    }
}
