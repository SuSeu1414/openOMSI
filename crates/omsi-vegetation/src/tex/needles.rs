//! Needle atlas tiles (spruce sprays, pine brushes): the twig skeleton of the leaf atlas with
//! thin tapered needles around each shoot, drawn far to near. Pure data, deterministic.

use super::leaf_raster::{hull_dist, lin, mix3, rot, Cap, Layer};
use super::leaves::DENSE_TILE;
use crate::classify::Archetype;
use crate::seed::Rng;
use glam::Vec2;

pub(super) struct NeedleStyle {
    /// Needle length, card units.
    pub len: f32,
    /// Needle half-width at the base (the tip is 0.4 x).
    pub radius: f32,
    /// Needle angle from the shoot axis (3D).
    pub angle_deg: f32,
    /// Distance between needle nodes along a shoot.
    pub spacing: f32,
    /// Shoots on a single-cluster tile and on the dense tile.
    pub shoots: (u32, u32),
    /// Fraction of each shoot from its base without needles.
    pub bare: f32,
    /// Pine: two needles per node (a fascicle).
    pub pair: bool,
    pub needle: [u8; 3],
    /// Fresh growth at the shoot ends.
    pub tip: [u8; 3],
    pub twig: [u8; 3],
}

pub(super) fn needle_style(a: Archetype) -> Option<NeedleStyle> {
    match a {
        Archetype::ConiferConical => Some(NeedleStyle {
            len: 0.035,
            radius: 0.0022,
            angle_deg: 60.0,
            spacing: 0.0035,
            shoots: (36, 160),
            bare: 0.0,
            pair: false,
            needle: [38, 62, 34],
            tip: [70, 100, 45],
            twig: [96, 70, 50],
        }),
        Archetype::Pine => Some(NeedleStyle {
            len: 0.16,
            radius: 0.0028,
            angle_deg: 35.0,
            spacing: 0.006,
            shoots: (11, 60),
            bare: 0.35,
            pair: true,
            needle: [70, 92, 52],
            tip: [90, 110, 60],
            twig: [120, 90, 60],
        }),
        _ => None,
    }
}

struct Needle {
    a: Vec2,
    b: Vec2,
    key: f32,
    col: [f32; 3],
}

/// Draws one tile: the twig first, then its needles far to near.
pub(super) fn needle_tile(st: &NeedleStyle, tile: u32, seed: u64, t: usize) -> Layer {
    let mut rng = Rng::new(seed.wrapping_add(tile as u64));
    let mut layer = Layer::new(t);
    let (shoots, k) = if tile == DENSE_TILE {
        (st.shoots.1, 0.7)
    } else {
        (st.shoots.0, 1.0)
    };
    let (nlen, spacing) = (st.len * k, st.spacing * k);
    let (twig, needle, tip) = (lin(st.twig), lin(st.needle), lin(st.tip));
    let bend = rng.signed() * 0.08;
    let stem = |t: f32| Vec2::new(0.5 + bend * t * t, 0.02 + 0.88 * t);
    let tangent = |t: f32| Vec2::new(2.0 * bend * t, 0.88).normalize();
    let jit = |rng: &mut Rng| twig.map(|c| c * (0.9 + 0.2 * rng.unit()));
    let mut caps: Vec<(Cap, [f32; 3])> = Vec::new();
    for k in 0..6 {
        let (t0, t1) = (k as f32 / 6.0, (k + 1) as f32 / 6.0);
        let r = |t: f32| 0.5 * (0.010 + (0.004 - 0.010) * t);
        let c = Cap {
            a: stem(t0),
            b: stem(t1),
            ra: r(t0),
            rb: r(t1),
        };
        caps.push((c, jit(&mut rng)));
    }
    let mut needles: Vec<Needle> = Vec::new();
    let mut counter = 0u32;
    // One node: one needle (two for a pine fascicle) around the shoot direction `d` at `p`;
    // `end` = 0..1 position along the shoot, for the tip colour.
    let mut node = |rng: &mut Rng, p: Vec2, d: Vec2, end: f32| {
        let phis: &[f32] = if st.pair { &[0.0, 20.0] } else { &[0.0] };
        for extra in phis {
            let phi = (counter as f32 * 137.508 + extra).to_radians();
            let alpha = (st.angle_deg + 10.0 * rng.signed()).to_radians();
            let l = nlen * (0.85 + 0.3 * rng.unit());
            let vec = |l: f32| {
                let perp = Vec2::new(-d.y, d.x);
                d * l * alpha.cos() + perp * l * alpha.sin() * phi.cos()
            };
            let key = phi.sin();
            let mix_t = if end > 0.8 { 0.6 } else { 0.0 };
            let jitter = 0.9 + 0.2 * rng.unit();
            let mut v = vec(l);
            if v.length() < 0.3 * l {
                continue;
            }
            if hull_dist(p + v) < 0.01 {
                v = vec(0.7 * l);
                if hull_dist(p + v) < 0.01 {
                    continue;
                }
            }
            let shade = (0.75 + 0.25 * (key + 1.0) / 2.0) * jitter;
            needles.push(Needle {
                a: p,
                b: p + v,
                key,
                col: mix3(needle, tip, mix_t).map(|c| c * shade),
            });
        }
        counter += 1;
    };
    let mut s_last = 0.08;
    for i in 0..shoots {
        let s = 0.08 + 0.77 * i as f32 / shoots as f32;
        s_last = s;
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let ang = (50.0 + 20.0 * rng.unit()).to_radians();
        let dir = rot(tangent(s), side * ang);
        let len = 0.25 + 0.15 * rng.unit();
        let base = stem(s);
        // The shoot curves: its direction turns by `bend` in total, quadratically.
        let bend = (20.0 * rng.signed()).to_radians();
        const N: usize = 16;
        let step = len / N as f32;
        let mut pts = vec![(base, dir)];
        for k in 0..N {
            let (p, _) = pts[k];
            let d = rot(dir, bend * ((k as f32 + 0.5) / N as f32).powi(2));
            pts.push((p + d * step, d));
        }
        for k in 0..N {
            let r = |k: usize| 0.00125 - 0.000625 * k as f32 / N as f32;
            let c = Cap {
                a: pts[k].0,
                b: pts[k + 1].0,
                ra: r(k),
                rb: r(k + 1),
            };
            caps.push((c, jit(&mut rng)));
        }
        let mut m = (st.bare * len / spacing).ceil().max(1.0);
        while m * spacing <= 0.97 * len {
            let dist = m * spacing;
            let k = ((dist / step) as usize).min(N - 1);
            let f = dist / step - k as f32;
            let p = pts[k].0.lerp(pts[k + 1].0, f);
            node(&mut rng, p, pts[k + 1].1, dist / len);
            m += 1.0;
        }
    }
    let mut s = s_last + spacing / 0.88;
    while s <= 1.0 {
        node(&mut rng, stem(s), tangent(s), s);
        s += spacing / 0.88;
    }
    for (c, col) in &caps {
        c.draw(&mut layer, *col, 0.7, 0.3, 0.85, 0.0);
    }
    needles.sort_by(|a, b| a.key.total_cmp(&b.key));
    for n in &needles {
        Cap {
            a: n.a,
            b: n.b,
            ra: st.radius,
            rb: 0.4 * st.radius,
        }
        .draw(&mut layer, n.col, 0.7, 0.3, 0.5, 0.35);
    }
    layer
}
