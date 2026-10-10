//! Procedural bark per archetype: a tileable albedo + height map and a normal + roughness + AO
//! map, built from the noise and field blocks. Pure data, deterministic.

use super::noise::{fbm, texel_p, worley};
use super::ops::{cavity_ao, encode_normal, normal_from_height, unorm8};
use super::Field;
use crate::classify::Archetype;
use crate::seed::splitmix64;
use glam::Vec2;

/// Metres of bark per texture repeat; equals the tube default `bark_tile`.
pub const BARK_TILE_M: f32 = 0.5;

/// Two RGBA8 maps, `size` x `size`, row-major, tileable in both directions.
/// `albedo_height`: rgb = albedo (sRGB bytes), a = height (0 = deepest furrow, 255 = top).
/// `normal_rough_ao`: r = roughness, g = normal.y, b = AO, a = normal.x (tangent space as
/// `ops::normal_from_height`: +x = u, +y = row/v; z rebuilt in the shader).
/// Tangent frame: +x = u (around the stem), +y = texture row = +v, N out of the surface, so
/// (T, B, N) is left-handed; the shader must use B = the +v direction, not cross(N, T).
pub struct BarkTextures {
    pub size: u32,
    pub albedo_height: Vec<u8>,
    pub normal_rough_ao: Vec<u8>,
}

#[derive(Clone, Copy)]
enum Kind {
    Furrowed,
    Plated,
    Scaly,
    Lenticel,
    Smooth,
}

struct Style {
    kind: Kind,
    /// Worley cells across one tile (Furrowed only).
    period: [u32; 2],
    edge: f32,
    warp: f32,
    /// sRGB colours; for Lenticel `ridge` is the white, `alt` the dark patch and `crevice` the lenticel dash.
    crevice: [u8; 3],
    ridge: [u8; 3],
    alt: [u8; 3],
    relief_m: f32,
    rough_ridge: f32,
    rough_crevice: f32,
}

/// Furrowed style: (period, edge, warp), colours (crevice, ridge), relief, roughness.
fn furrowed(
    shape: ([u32; 2], f32, f32),
    colours: ([u8; 3], [u8; 3]),
    relief_m: f32,
    rough: (f32, f32),
) -> Style {
    plain(Kind::Furrowed, colours, [0; 3], relief_m, rough, shape)
}

fn plain(
    kind: Kind,
    colours: ([u8; 3], [u8; 3]),
    alt: [u8; 3],
    relief_m: f32,
    rough: (f32, f32),
    shape: ([u32; 2], f32, f32),
) -> Style {
    Style {
        kind,
        period: shape.0,
        edge: shape.1,
        warp: shape.2,
        crevice: colours.0,
        ridge: colours.1,
        alt,
        relief_m,
        rough_ridge: rough.0,
        rough_crevice: rough.1,
    }
}

fn style(a: Archetype) -> Option<Style> {
    let none = ([1, 1], 0.0, 0.0);
    Some(match a {
        Archetype::BroadleafWide => furrowed(
            ([10, 2], 0.55, 0.25),
            ([38, 32, 27], [112, 104, 94]),
            0.012,
            (0.85, 0.95),
        ),
        Archetype::BroadleafNarrow => furrowed(
            ([9, 2], 0.45, 0.20),
            ([40, 36, 31], [128, 122, 110]),
            0.014,
            (0.85, 0.95),
        ),
        Archetype::Willow => furrowed(
            ([12, 2], 0.65, 0.45),
            ([34, 29, 24], [105, 95, 80]),
            0.018,
            (0.88, 0.96),
        ),
        Archetype::Pine => plain(
            Kind::Plated,
            ([52, 32, 24], [150, 92, 62]),
            [98, 82, 70],
            0.014,
            (0.80, 0.95),
            none,
        ),
        Archetype::ConiferConical => plain(
            Kind::Scaly,
            ([45, 33, 27], [110, 82, 64]),
            [0; 3],
            0.006,
            (0.82, 0.92),
            none,
        ),
        Archetype::Birch => plain(
            Kind::Lenticel,
            ([62, 54, 50], [222, 218, 206]),
            [34, 30, 28],
            0.002,
            (0.55, 0.85),
            none,
        ),
        Archetype::Shrub => plain(
            Kind::Smooth,
            ([50, 42, 34], [104, 92, 78]),
            [0; 3],
            0.003,
            (0.75, 0.85),
            none,
        ),
        Archetype::Hedge => plain(
            Kind::Smooth,
            ([46, 40, 32], [96, 86, 72]),
            [0; 3],
            0.003,
            (0.75, 0.85),
            none,
        ),
        Archetype::None => return None,
    })
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lin(c: [u8; 3]) -> [f32; 3] {
    c.map(|v| (v as f32 / 255.0).powf(2.2))
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// Height (normalised to 0..1 over the tile) and per-cell tint (0..1; darkness for Lenticel).
fn height(s: &Style, size: u32, seed: u64) -> (Field, Field) {
    let mut tint = Field::new(size);
    let mut h = Field::new(size);
    let per = s.period;
    for y in 0..size {
        for x in 0..size {
            let i = (y * size + x) as usize;
            let p = |period| texel_p(x, y, size, period);
            let (v, t) = match s.kind {
                Kind::Furrowed => {
                    let q = p(per);
                    let w = Vec2::new(
                        fbm(q, per, 4, seed.wrapping_add(1)) - 0.5,
                        fbm(q, per, 4, seed.wrapping_add(2)) - 0.5,
                    );
                    let c = worley(q + w * Vec2::new(1.0, 0.3) * s.warp, per, seed);
                    let r = smoothstep(0.0, s.edge, c.f2 - c.f1);
                    let p4 = [per[0] * 4, per[1] * 4];
                    (
                        0.75 * r + 0.25 * fbm(p(p4), p4, 5, seed.wrapping_add(3)),
                        c.id,
                    )
                }
                Kind::Plated => {
                    let c = worley(p([4, 3]), [4, 3], seed);
                    let plate = smoothstep(0.04, 0.20, c.f2 - c.f1);
                    let terr =
                        (fbm(p([8, 6]), [8, 6], 3, seed.wrapping_add(4)) * 3.0).floor() / 3.0;
                    (
                        0.75 * plate
                            + 0.25 * fbm(p([16, 12]), [16, 12], 4, seed.wrapping_add(3))
                            + 0.1 * terr * plate,
                        c.id,
                    )
                }
                Kind::Scaly => {
                    let c = worley(p([12, 9]), [12, 9], seed);
                    let scale = 1.0 - (c.f1 / 0.7).clamp(0.0, 1.0).powi(2);
                    (
                        0.7 * scale + 0.3 * fbm(p([24, 18]), [24, 18], 4, seed.wrapping_add(3)),
                        c.id,
                    )
                }
                Kind::Lenticel => {
                    let c = worley(p([3, 22]), [3, 22], seed);
                    let dash = if c.f1 < 0.08 + 0.14 * c.id {
                        1.0f32
                    } else {
                        0.0
                    };
                    let patch =
                        smoothstep(0.57, 0.63, fbm(p([2, 6]), [2, 6], 5, seed.wrapping_add(5)));
                    (
                        0.6 + 0.15 * fbm(p([6, 6]), [6, 6], 4, seed.wrapping_add(3))
                            - 0.35 * dash
                            - 0.15 * patch,
                        // ponytail: tint > 1 marks a dash (drawn in `crevice`), else patch darkness
                        if dash > 0.0 { 2.0 } else { patch },
                    )
                }
                Kind::Smooth => (
                    0.5 + 0.35 * fbm(p([8, 3]), [8, 3], 5, seed.wrapping_add(3))
                        + 0.15 * fbm(p([32, 32]), [32, 32], 3, seed.wrapping_add(4)),
                    fbm(p([4, 4]), [4, 4], 3, seed.wrapping_add(6)),
                ),
            };
            h.v[i] = v;
            tint.v[i] = t;
        }
    }
    let (lo, hi) =
        h.v.iter()
            .fold((f32::MAX, f32::MIN), |(l, u), &v| (l.min(v), u.max(v)));
    let k = 1.0 / (hi - lo).max(1e-6);
    h.v.iter_mut().for_each(|v| *v = (*v - lo) * k);
    (h, tint)
}

/// `None` for `Archetype::None`. `size` a power of two >= 32. Deterministic: the seed is
/// fixed per archetype (`splitmix64(archetype index ^ 0xBA4C)`).
pub fn bark(a: Archetype, size: u32) -> Option<BarkTextures> {
    assert!(
        size >= 32 && size.is_power_of_two(),
        "bark: bad size {size}"
    );
    let s = style(a)?;
    let seed = splitmix64(a as u64 ^ 0xBA4C);
    let (h, tint) = height(&s, size, seed);
    let strength = s.relief_m / (BARK_TILE_M / size as f32);
    let normals = normal_from_height(&h, strength, true);
    let ao = cavity_ao(&h, (size / 64).max(1), 2.0, true);
    let (crev, ridge, alt) = (lin(s.crevice), lin(s.ridge), lin(s.alt));
    let n = (size * size) as usize;
    let mut albedo_height = Vec::with_capacity(n * 4);
    let mut nra = Vec::with_capacity(n * 4);
    for y in 0..size {
        for x in 0..size {
            let i = (y * size + x) as usize;
            let (hv, t) = (h.v[i], tint.v[i]);
            let grain = 0.9
                + 0.2
                    * fbm(
                        texel_p(x, y, size, [64, 64]),
                        [64, 64],
                        3,
                        seed.wrapping_add(7),
                    );
            let base = match s.kind {
                Kind::Lenticel if t > 1.0 => crev,
                Kind::Lenticel => mix3(ridge, alt, 0.8 * t),
                Kind::Plated => mix3(mix3(crev, ridge, hv), alt, smoothstep(0.3, 0.9, t)),
                _ => mix3(crev, ridge, hv).map(|c| c * (0.85 + 0.3 * t)),
            };
            for c in base {
                albedo_height.push(unorm8((c * grain).clamp(0.0, 1.0).powf(1.0 / 2.2)));
            }
            albedo_height.push(unorm8(hv));
            let rough = s.rough_crevice + (s.rough_ridge - s.rough_crevice) * hv;
            let grit = fbm(
                texel_p(x, y, size, [32, 32]),
                [32, 32],
                3,
                seed.wrapping_add(8),
            ) - 0.5;
            let e = encode_normal(normals[i]);
            nra.extend_from_slice(&[
                unorm8(rough + 0.04 * grit),
                e[1],
                unorm8(ao.v[i] * (0.75 + 0.25 * hv)),
                e[0],
            ]);
        }
    }
    Some(BarkTextures {
        size,
        albedo_height,
        normal_rough_ao: nra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Archetype; 8] = [
        Archetype::BroadleafWide,
        Archetype::BroadleafNarrow,
        Archetype::ConiferConical,
        Archetype::Pine,
        Archetype::Willow,
        Archetype::Birch,
        Archetype::Shrub,
        Archetype::Hedge,
    ];

    #[test]
    fn bark_deterministic() {
        for a in ALL {
            let (x, y) = (bark(a, 64).unwrap(), bark(a, 64).unwrap());
            assert_eq!(x.albedo_height, y.albedo_height, "{a:?}");
            assert_eq!(x.normal_rough_ao, y.normal_rough_ao, "{a:?}");
        }
        assert!(bark(Archetype::None, 64).is_none());
    }

    #[test]
    fn bark_tiles() {
        let n = 64usize;
        for a in ALL {
            let t = bark(a, 64).unwrap();
            for m in [&t.albedo_height, &t.normal_rough_ao] {
                let diff = |p: (usize, usize), q: (usize, usize)| -> f32 {
                    let at = |(x, y): (usize, usize)| &m[(y * n + x) * 4..(y * n + x) * 4 + 4];
                    at(p)
                        .iter()
                        .zip(at(q))
                        .map(|(a, b)| a.abs_diff(*b) as f32)
                        .sum::<f32>()
                        / 4.0
                };
                let (mut seam, mut inner) = (0.0, 0.0);
                for k in 0..n {
                    seam += diff((k, n - 1), (k, 0)) + diff((n - 1, k), (0, k));
                    for j in 0..n - 1 {
                        inner += diff((k, j), (k, j + 1)) + diff((j, k), (j + 1, k));
                    }
                }
                let (seam, inner) = (seam / (2 * n) as f32, inner / (2 * n * (n - 1)) as f32);
                assert!(
                    seam <= 2.0 * inner + 2.0,
                    "{a:?}: seam {seam} inner {inner}"
                );
            }
        }
    }

    #[test]
    fn bark_height_uses_range() {
        for a in ALL {
            let t = bark(a, 64).unwrap();
            let h = t.albedo_height.chunks(4).map(|p| p[3]);
            assert!(h.clone().min().unwrap() <= 20, "{a:?}");
            assert!(h.max().unwrap() >= 235, "{a:?}");
        }
    }

    #[test]
    fn bark_normals_valid() {
        for a in ALL {
            for p in bark(a, 64).unwrap().normal_rough_ao.chunks(4) {
                let (y, x) = (p[1] as f32 / 127.5 - 1.0, p[3] as f32 / 127.5 - 1.0);
                assert!(x * x + y * y <= 1.02, "{a:?}");
            }
        }
    }

    #[test]
    fn birch_is_light_pine_is_warm() {
        let mean = |a, f: fn(&[u8]) -> f32| {
            let t = bark(a, 64).unwrap();
            t.albedo_height.chunks(4).map(f).sum::<f32>() / 4096.0
        };
        let lum = |p: &[u8]| 0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32;
        assert!(mean(Archetype::Birch, lum) > 150.0);
        let r = mean(Archetype::Pine, |p| p[0] as f32);
        let b = mean(Archetype::Pine, |p| p[2] as f32);
        assert!(r > b + 25.0, "{r} {b}");
    }
}
