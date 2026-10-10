//! Procedural leaf atlas per archetype (broadleaf kinds): 2x2 tiles of twig clusters drawn
//! inside the fixed card outline `generator::leaves::HULL`. Pure data, deterministic.

use super::ops::{dilate, encode_normal, unorm8};
use crate::classify::Archetype;
use crate::generator::leaves::HULL;
use crate::seed::{splitmix64, Rng};
use glam::{Vec2, Vec3};
use std::f32::consts::{PI, TAU};

/// Tiles per atlas side (2x2). Tiles 0..=2: single twig clusters for LOD0 cards (picked by
/// the card's rand); tile 3: a dense cluster for the bigger LOD1/LOD2 cards.
pub const TILES: u32 = 2;
pub const DENSE_TILE: u32 = 3;
/// Gutter between the hull and the tile edge, as a fraction of the tile side.
pub const MARGIN: f32 = 1.0 / 64.0;

/// Atlas uv (0..1, top-left origin) of a card-space uv on `tile`; the renderer uses the same
/// formula. Card base (v = 0) at the tile's bottom, tip at its top.
pub fn atlas_uv(card: Vec2, tile: u32) -> Vec2 {
    let (col, row) = ((tile % TILES) as f32, (tile / TILES) as f32);
    let k = 1.0 - 2.0 * MARGIN;
    Vec2::new(
        (col + MARGIN + card.x * k) / TILES as f32,
        (row + MARGIN + (1.0 - card.y) * k) / TILES as f32,
    )
}

/// `albedo_alpha`: rgb = albedo of the leaf top (sRGB bytes), a = coverage.
/// `normal_rough_trans`: r = roughness, g = normal.y, b = translucency (0 = opaque wood of
/// the twig, so the season recolour skips it), a = normal.x. Both `size` x `size` RGBA8.
/// Normals follow `ops::normal_from_height` (+x = atlas column, +y = atlas row, i.e. -v of
/// the card; z rebuilt in the shader), the same convention as the bark maps.
pub struct LeafTextures {
    pub size: u32,
    pub albedo_alpha: Vec<u8>,
    pub normal_rough_trans: Vec<u8>,
}

#[derive(Clone, Copy)]
enum Shape {
    Ovate,
    Elliptic,
    Rhombic,
    Lanceolate,
}

struct Style {
    shape: Shape,
    /// Leaf length in card units.
    len: f32,
    /// Half-width / length.
    w: f32,
    serr: f32,
    teeth: f32,
    /// Shoots on a single-cluster tile and on the dense tile.
    shoots: (u32, u32),
    spacing: f32,
    angle_deg: f32,
    petiole: f32,
    leaf: [u8; 3],
    twig: [u8; 3],
}

fn style(a: Archetype) -> Option<Style> {
    use Shape::*;
    #[allow(clippy::too_many_arguments)]
    fn st(
        shape: Shape,
        (len, w): (f32, f32),
        (serr, teeth): (f32, f32),
        shoots: (u32, u32),
        (spacing, angle_deg, petiole): (f32, f32, f32),
        leaf: [u8; 3],
        twig: [u8; 3],
    ) -> Option<Style> {
        Some(Style {
            shape,
            len,
            w,
            serr,
            teeth,
            shoots,
            spacing,
            angle_deg,
            petiole,
            leaf,
            twig,
        })
    }
    match a {
        Archetype::BroadleafWide => st(
            Ovate,
            (0.13, 0.42),
            (0.03, 22.0),
            (6, 22),
            (0.07, 55.0, 0.03),
            [74, 104, 38],
            [92, 74, 58],
        ),
        Archetype::BroadleafNarrow => st(
            Rhombic,
            (0.14, 0.45),
            (0.04, 18.0),
            (6, 22),
            (0.08, 50.0, 0.05),
            [80, 110, 40],
            [104, 96, 84],
        ),
        Archetype::Birch => st(
            Rhombic,
            (0.10, 0.38),
            (0.06, 26.0),
            (7, 32),
            (0.05, 60.0, 0.025),
            [92, 124, 44],
            [70, 52, 40],
        ),
        Archetype::Willow => st(
            Lanceolate,
            (0.10, 0.10),
            (0.02, 30.0),
            (9, 56),
            (0.02, 22.0, 0.005),
            [98, 118, 62],
            [120, 104, 60],
        ),
        Archetype::Shrub => st(
            Ovate,
            (0.13, 0.40),
            (0.05, 20.0),
            (7, 24),
            (0.06, 55.0, 0.02),
            [70, 98, 36],
            [86, 70, 54],
        ),
        Archetype::Hedge => st(
            Elliptic,
            (0.12, 0.35),
            (0.02, 24.0),
            (8, 32),
            (0.045, 60.0, 0.01),
            [56, 86, 32],
            [86, 70, 54],
        ),
        _ => None,
    }
}

/// The summer reference colour of the leaf top (sRGB) — the palette's `ref` in 024.
pub fn leaf_ref(a: Archetype) -> Option<[u8; 3]> {
    style(a).map(|s| s.leaf)
}

fn lin(c: [u8; 3]) -> [f32; 3] {
    c.map(|v| (v as f32 / 255.0).powf(2.2))
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// Signed distance to the nearest hull edge line (card units): >= 0 inside, exact inside,
/// a lower bound of the distance outside.
fn hull_dist(p: Vec2) -> f32 {
    let n = HULL.len();
    (0..n)
        .map(|i| {
            let (a, b) = (HULL[i], HULL[(i + 1) % n]);
            let e = b - a;
            e.perp_dot(p - a) / e.length()
        })
        .fold(f32::MAX, f32::min)
}

fn shape_w(s: &Style, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let w = match s.shape {
        Shape::Ovate => (PI * t.powf(0.75)).sin().max(0.0).powf(1.5),
        Shape::Elliptic => (PI * t).sin().max(0.0).powf(1.3),
        Shape::Rhombic => (if t < 0.4 { t / 0.4 } else { (1.0 - t) / 0.6 })
            .max(0.0)
            .powf(0.8),
        Shape::Lanceolate => (PI * t).sin().max(0.0).powf(0.7) * (1.0 - 0.35 * t),
    };
    s.w * w
}

/// One painted texel.
struct Texel {
    col: [f32; 3],
    h: f32,
    rough: f32,
    trans: f32,
}

const SUB: usize = 4;

/// Per-tile float layers, premultiplied by coverage (`over` compositing).
struct Layer {
    t: usize,
    a: Vec<f32>,
    col: Vec<[f32; 3]>,
    h: Vec<f32>,
    rough: Vec<f32>,
    trans: Vec<f32>,
}

impl Layer {
    fn new(t: usize) -> Layer {
        Layer {
            t,
            a: vec![0.0; t * t],
            col: vec![[0.0; 3]; t * t],
            h: vec![0.0; t * t],
            rough: vec![0.0; t * t],
            trans: vec![0.0; t * t],
        }
    }

    /// Card space of a position in tile pixels.
    fn card_at(&self, x: f32, y: f32) -> Vec2 {
        let k = 1.0 - 2.0 * MARGIN;
        let t = self.t as f32;
        Vec2::new((x / t - MARGIN) / k, 1.0 - (y / t - MARGIN) / k)
    }

    fn px_of(&self, c: Vec2) -> Vec2 {
        atlas_uv(c, 0) * TILES as f32 * self.t as f32
    }

    /// Coverage by 4x4 subsamples of `inside` per pixel of the card-space box `lo..hi`.
    fn paint(
        &mut self,
        (lo, hi): (Vec2, Vec2),
        inside: &dyn Fn(Vec2) -> bool,
        shade: &dyn Fn(Vec2) -> Texel,
    ) {
        let (pa, pb) = (self.px_of(lo), self.px_of(hi));
        let t = self.t as i32;
        let x0 = (pa.x.min(pb.x).floor() as i32).clamp(0, t);
        let x1 = (pa.x.max(pb.x).ceil() as i32).clamp(0, t);
        let y0 = (pa.y.min(pb.y).floor() as i32).clamp(0, t);
        let y1 = (pa.y.max(pb.y).ceil() as i32).clamp(0, t);
        for y in y0..y1 {
            for x in x0..x1 {
                let mut n = 0;
                for j in 0..SUB {
                    for i in 0..SUB {
                        let (sx, sy) = (
                            x as f32 + (i as f32 + 0.5) / SUB as f32,
                            y as f32 + (j as f32 + 0.5) / SUB as f32,
                        );
                        n += inside(self.card_at(sx, sy)) as u32;
                    }
                }
                if n == 0 {
                    continue;
                }
                let an = n as f32 / (SUB * SUB) as f32;
                let tx = shade(self.card_at(x as f32 + 0.5, y as f32 + 0.5));
                let i = y as usize * self.t + x as usize;
                let keep = 1.0 - an;
                for c in 0..3 {
                    self.col[i][c] = tx.col[c] * an + self.col[i][c] * keep;
                }
                self.h[i] = tx.h * an + self.h[i] * keep;
                self.rough[i] = tx.rough * an + self.rough[i] * keep;
                self.trans[i] = tx.trans * an + self.trans[i] * keep;
                self.a[i] = an + self.a[i] * keep;
            }
        }
    }
}

/// A tapered stem segment.
struct Cap {
    a: Vec2,
    b: Vec2,
    ra: f32,
    rb: f32,
}

impl Cap {
    /// Distance to the axis over the local radius (<= 1 inside).
    fn rel(&self, p: Vec2) -> f32 {
        let ab = self.b - self.a;
        let t = ((p - self.a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
        (p - (self.a + ab * t)).length() / (self.ra + (self.rb - self.ra) * t).max(1e-6)
    }

    fn bbox(&self) -> (Vec2, Vec2) {
        let r = Vec2::splat(self.ra.max(self.rb));
        (self.a.min(self.b) - r, self.a.max(self.b) + r)
    }

    fn draw(&self, l: &mut Layer, col: [f32; 3], h_axis: f32, h_edge: f32, rough: f32, tr: f32) {
        l.paint(self.bbox(), &|p| self.rel(p) <= 1.0, &|p| Texel {
            col,
            h: h_axis + (h_edge - h_axis) * self.rel(p).min(1.0),
            rough,
            trans: tr,
        });
    }
}

struct Leaf {
    attach: Vec2,
    b: Vec2,
    d: Vec2,
    l: f32,
    /// Width multiplier (leaf turned away from the viewer).
    wk: f32,
    tint: [f32; 3],
    rough: f32,
    key: f32,
}

fn rot(v: Vec2, a: f32) -> Vec2 {
    Vec2::from_angle(a).rotate(v)
}

/// Leaf frame: s along the leaf (0 base, 1 tip), t across, both in units of the length.
fn frame(lf: &Leaf, p: Vec2) -> (f32, f32) {
    let q = p - lf.b;
    (q.dot(lf.d) / lf.l, lf.d.perp_dot(q) / (lf.l * lf.wk))
}

fn half_width(st: &Style, s: f32) -> f32 {
    shape_w(st, s) * (1.0 + st.serr * (s * TAU * st.teeth).sin())
}

fn draw_leaf(l: &mut Layer, st: &Style, lf: &Leaf, shade: f32) {
    let leaf = lin(st.leaf);
    let perp = Vec2::new(-lf.d.y, lf.d.x);
    let wl = st.w * lf.l * 1.15;
    let tip = lf.b + lf.d * lf.l;
    let pts = [
        lf.b + perp * wl,
        lf.b - perp * wl,
        tip + perp * wl,
        tip - perp * wl,
    ];
    let lo = pts.iter().fold(Vec2::MAX, |m, p| m.min(*p));
    let hi = pts.iter().fold(Vec2::MIN, |m, p| m.max(*p));
    l.paint(
        (lo, hi),
        &|p| {
            let (s, t) = frame(lf, p);
            (0.0..=1.0).contains(&s) && t.abs() <= half_width(st, s)
        },
        &|p| {
            let (s, t) = frame(lf, p);
            let s = s.clamp(0.0, 1.0);
            let w = half_width(st, s);
            let rel = (t.abs() / w.max(1e-4)).min(1.0);
            let g = (-(t / (0.05 * st.w)).powi(2)).exp();
            let vn = smoothstep(0.9, 1.0, ((s - 1.4 * t.abs() / st.w) * 7.0).rem_euclid(1.0));
            let k = (1.0 + 0.12 * (g + vn)) * (1.0 - 0.15 * rel.powi(4)) * shade;
            Texel {
                col: [0, 1, 2].map(|i| leaf[i] * lf.tint[i] * k),
                h: 0.5 + 0.35 * (1.0 - rel * rel) - 0.15 * g - 0.06 * vn,
                rough: 0.42 + 0.1 * vn + lf.rough,
                trans: 0.8 - 0.25 * (g + vn).min(1.0),
            }
        },
    );
}

/// Whether the leaf outline (both edges at five stations plus base and tip) lies inside the
/// hull inset by 0.01; tests exactly the length that is drawn.
fn leaf_fits(st: &Style, b: Vec2, d: Vec2, l: f32, wk: f32) -> bool {
    let perp = Vec2::new(-d.y, d.x);
    let ok = |p: Vec2| hull_dist(p) >= 0.01;
    ok(b)
        && ok(b + d * l)
        && [0.15, 0.3, 0.45, 0.65, 0.85].iter().all(|&s| {
            let w = perp * half_width(st, s) * l * wk * 1.1;
            ok(b + d * (s * l) + w) && ok(b + d * (s * l) - w)
        })
}

/// Draws one tile: the twig first, then its leaves back to front.
fn tile_layer(st: &Style, tile: u32, seed: u64, t: usize) -> Layer {
    let mut rng = Rng::new(seed.wrapping_add(tile as u64));
    let mut layer = Layer::new(t);
    let dense = tile == DENSE_TILE;
    let (shoots, k) = if dense {
        (st.shoots.1, 0.6)
    } else {
        (st.shoots.0, 1.0)
    };
    let (leaf_len, spacing) = (st.len * k, st.spacing * k);
    let twig = lin(st.twig);
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
    // (attach point, direction, length) of every shoot, then the leaves on them.
    let mut leaves: Vec<Leaf> = Vec::new();
    let mut place = |rng: &mut Rng, attach: Vec2, dir: Vec2, side: f32, extra: f32| {
        let ang = (st.angle_deg + extra + 25.0 * rng.signed()).to_radians();
        let d = rot(dir, side * ang);
        let b = attach + d * st.petiole;
        let mut l = leaf_len * (0.75 + 0.5 * rng.unit());
        let wk = 0.55 + 0.45 * rng.unit();
        let mut ok = leaf_fits(st, b, d, l, wk);
        if !ok {
            l *= 0.7;
            ok = leaf_fits(st, b, d, l, wk);
        }
        let tint = mix3([1.0; 3], [1.15, 1.05, 0.7], rng.unit() * 0.25)
            .map(|c| c * (0.88 + 0.24 * rng.unit()));
        let (rough, key) = (0.08 * rng.unit(), rng.unit());
        if ok {
            leaves.push(Leaf {
                attach,
                b,
                d,
                l,
                wk,
                tint,
                rough,
                key,
            });
        }
    };
    let mut s_last = 0.08;
    for i in 0..shoots {
        let s = 0.08 + 0.77 * i as f32 / shoots as f32;
        s_last = s;
        let side = if i % 2 == 0 { 1.0 } else { -1.0 };
        let ang = (35.0 + 25.0 * rng.unit()).to_radians();
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
        let at = |dist: f32| {
            let k = ((dist / step) as usize).min(N - 1);
            let f = dist / step - k as f32;
            (pts[k].0.lerp(pts[k + 1].0, f), pts[k + 1].1)
        };
        let mut m = 1.0;
        while m * spacing <= len * 0.95 {
            let (p, d) = at(m * spacing);
            for side in [1.0, -1.0] {
                place(&mut rng, p, d, side, 0.0);
            }
            // Second, overlapping pass at half spacing, turned further out.
            if (m + 0.5) * spacing <= len * 0.95 {
                let (p, d) = at((m + 0.5) * spacing);
                for side in [1.0, -1.0] {
                    place(&mut rng, p, d, side, 25.0);
                }
            }
            m += 1.0;
        }
    }
    let mut s = s_last + spacing / 0.88;
    while s <= 1.0 {
        for side in [1.0, -1.0] {
            place(&mut rng, stem(s), tangent(s), side, 0.0);
        }
        s += spacing / 0.88;
    }
    for (c, col) in &caps {
        c.draw(&mut layer, *col, 0.7, 0.3, 0.85, 0.0);
    }
    leaves.sort_by(|a, b| a.key.total_cmp(&b.key));
    let n = leaves.len() as f32;
    for (k, lf) in leaves.iter().enumerate() {
        let col = mix3(twig, lin(st.leaf), 0.5);
        Cap {
            a: lf.attach,
            b: lf.b,
            ra: 0.003,
            rb: 0.003,
        }
        .draw(&mut layer, col, 0.6, 0.4, 0.7, 0.3);
        draw_leaf(&mut layer, st, lf, 0.72 + 0.28 * (k as f32 / n));
    }
    layer
}

/// Unpremultiplies a layer, applies the hull coverage and packs both maps of the tile.
fn pack_tile(l: &Layer) -> (Vec<[u8; 4]>, Vec<[u8; 4]>) {
    let t = l.t;
    let mut hull = vec![0.0f32; t * t];
    for (i, hc) in hull.iter_mut().enumerate() {
        let (x, y) = ((i % t) as f32, (i / t) as f32);
        let mut n = 0;
        for j in 0..SUB {
            for k in 0..SUB {
                let p = l.card_at(
                    x + (k as f32 + 0.5) / SUB as f32,
                    y + (j as f32 + 0.5) / SUB as f32,
                );
                n += (hull_dist(p) >= 0.0) as u32;
            }
        }
        *hc = n as f32 / (SUB * SUB) as f32;
    }
    let a_fin: Vec<f32> = l.a.iter().zip(&hull).map(|(a, h)| a * h).collect();
    // Unpremultiplied height; a neighbour with alpha < 0.5 counts as the centre texel, so the
    // silhouette has no height cliff and no bevel rim. Same convention as `normal_from_height`.
    let hgt = |i: usize| if l.a[i] > 1e-6 { l.h[i] / l.a[i] } else { 0.0 };
    let strength = 3.0 * t as f32 / 512.0;
    let at = |x: i32, y: i32, c: usize| {
        let (x, y) = (
            x.clamp(0, t as i32 - 1) as usize,
            y.clamp(0, t as i32 - 1) as usize,
        );
        let i = y * t + x;
        if a_fin[i] < 0.5 {
            hgt(c)
        } else {
            hgt(i)
        }
    };
    let normals: Vec<Vec3> = (0..t * t)
        .map(|c| {
            let (x, y) = ((c % t) as i32, (c / t) as i32);
            let dx = (at(x + 1, y, c) - at(x - 1, y, c)) * 0.5;
            let dy = (at(x, y + 1, c) - at(x, y - 1, c)) * 0.5;
            Vec3::new(-strength * dx, -strength * dy, 1.0).normalize()
        })
        .collect();
    let (mut alb, mut nrt) = (Vec::with_capacity(t * t), Vec::with_capacity(t * t));
    for i in 0..t * t {
        let inv = if l.a[i] > 1e-6 { 1.0 / l.a[i] } else { 0.0 };
        let c = l.col[i].map(|c| unorm8((c * inv).clamp(0.0, 1.0).powf(1.0 / 2.2)));
        alb.push([c[0], c[1], c[2], unorm8(a_fin[i])]);
        let e = encode_normal(normals[i]);
        nrt.push([
            unorm8(l.rough[i] * inv),
            e[1],
            unorm8(l.trans[i] * inv),
            e[0],
        ]);
    }
    (alb, nrt)
}

/// `None` for `Archetype::None` and (until 023b) the needle archetypes. `size` a power of two
/// >= 128. Deterministic (seed per archetype, `splitmix64(index ^ 0x1EAF)`).
pub fn leaves(a: Archetype, size: u32) -> Option<LeafTextures> {
    assert!(
        size >= 128 && size.is_power_of_two(),
        "leaves: bad size {size}"
    );
    let st = style(a)?;
    let seed = splitmix64(a as u64 ^ 0x1EAF);
    let t = (size / TILES) as usize;
    let n = size as usize;
    let mut alb = vec![0u8; n * n * 4];
    let mut nrt = vec![0u8; n * n * 4];
    for tile in 0..TILES * TILES {
        let (a_t, n_t) = pack_tile(&tile_layer(&st, tile, seed, t));
        let (ox, oy) = ((tile % TILES) as usize * t, (tile / TILES) as usize * t);
        for i in 0..t * t {
            let o = ((oy + i / t) * n + ox + i % t) * 4;
            alb[o..o + 4].copy_from_slice(&a_t[i]);
            nrt[o..o + 4].copy_from_slice(&n_t[i]);
        }
    }
    let mask: Vec<u8> = alb.chunks(4).map(|p| p[3]).collect();
    dilate(&mut alb, &mask, size, 3, 16);
    dilate(&mut nrt, &mask, size, 4, 16);
    Some(LeafTextures {
        size,
        albedo_alpha: alb,
        normal_rough_trans: nrt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BROAD: [Archetype; 6] = [
        Archetype::BroadleafWide,
        Archetype::BroadleafNarrow,
        Archetype::Birch,
        Archetype::Willow,
        Archetype::Shrub,
        Archetype::Hedge,
    ];
    const S: u32 = 256;

    /// Card-space position of the centre of atlas texel (x, y) on its tile.
    fn card_of(x: u32, y: u32) -> (u32, Vec2) {
        let t = (S / TILES) as f32;
        let (tile, lx, ly) = (
            (y / (S / TILES)) * TILES + x / (S / TILES),
            ((x % (S / TILES)) as f32 + 0.5) / t,
            ((y % (S / TILES)) as f32 + 0.5) / t,
        );
        let k = 1.0 - 2.0 * MARGIN;
        (tile, Vec2::new((lx - MARGIN) / k, 1.0 - (ly - MARGIN) / k))
    }

    #[test]
    fn leaves_deterministic() {
        for a in BROAD {
            let (x, y) = (leaves(a, S).unwrap(), leaves(a, S).unwrap());
            assert_eq!(x.albedo_alpha, y.albedo_alpha, "{a:?}");
            assert_eq!(x.normal_rough_trans, y.normal_rough_trans, "{a:?}");
        }
        for a in [Archetype::None, Archetype::ConiferConical, Archetype::Pine] {
            assert!(leaves(a, S).is_none());
        }
    }

    #[test]
    fn nothing_outside_hull() {
        for a in BROAD {
            let t = leaves(a, S).unwrap();
            for y in 0..S {
                for x in 0..S {
                    let (_, c) = card_of(x, y);
                    let px = -hull_dist(c) * (1.0 - 2.0 * MARGIN) * (S / TILES) as f32;
                    if px > 1.5 {
                        assert_eq!(t.albedo_alpha[((y * S + x) * 4 + 3) as usize], 0, "{a:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn hull_fill_in_range() {
        for a in BROAD {
            let t = leaves(a, S).unwrap();
            let (mut hull, mut cov) = ([0u32; 4], [0u32; 4]);
            for y in 0..S {
                for x in 0..S {
                    let (tile, c) = card_of(x, y);
                    if hull_dist(c) >= 0.0 {
                        hull[tile as usize] += 1;
                        cov[tile as usize] +=
                            (t.albedo_alpha[((y * S + x) * 4 + 3) as usize] > 128) as u32;
                    }
                }
            }
            for k in 0..4 {
                let f = cov[k] as f32 / hull[k] as f32;
                if k < 3 {
                    assert!((0.35..=0.85).contains(&f), "{a:?} tile {k}: {f}");
                } else {
                    assert!(f >= 0.60, "{a:?} dense tile: {f}");
                }
            }
        }
    }

    #[test]
    fn no_black_under_alpha_zero() {
        for a in BROAD {
            let t = leaves(a, S).unwrap();
            let (mut sum, mut n) = ([0f32; 3], 0f32);
            for p in t.albedo_alpha.chunks(4).filter(|p| p[3] > 0) {
                (0..3).for_each(|i| sum[i] += p[i] as f32);
                n += 1.0;
            }
            let mean = sum.map(|s| s / n);
            for p in t.albedo_alpha.chunks(4).filter(|p| p[3] == 0) {
                for i in 0..3 {
                    assert!(
                        (p[i] as f32 - mean[i]).abs() <= 70.0,
                        "{a:?}: {p:?} vs {mean:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn twig_is_opaque_wood() {
        for a in BROAD {
            let t = leaves(a, 512).unwrap();
            let opaque: Vec<usize> = (0..512 * 512)
                .filter(|&i| t.albedo_alpha[i * 4 + 3] == 255)
                .collect();
            let wood = opaque.iter().any(|&i| t.normal_rough_trans[i * 4 + 2] == 0);
            let mean = opaque
                .iter()
                .map(|&i| t.normal_rough_trans[i * 4 + 2] as f32)
                .sum::<f32>()
                / opaque.len() as f32;
            assert!(wood, "{a:?}: no wood texel");
            assert!(mean > 100.0, "{a:?}: mean translucency {mean}");
        }
    }

    #[test]
    fn single_leaf_has_its_tip() {
        let st = style(Archetype::BroadleafWide).unwrap();
        let mut layer = Layer::new(256);
        let (b, l) = (Vec2::new(0.5, 0.3), 0.3);
        let lf = Leaf {
            attach: b,
            b,
            d: Vec2::Y,
            l,
            wk: 1.0,
            tint: [1.0; 3],
            rough: 0.0,
            key: 0.0,
        };
        draw_leaf(&mut layer, &st, &lf, 1.0);
        let alpha = |s: f32| {
            let p = layer.px_of(b + Vec2::Y * (s * l));
            layer.a[p.y as usize * 256 + p.x as usize]
        };
        assert!(alpha(0.9) > 0.5, "tip region empty: {}", alpha(0.9));
        assert_eq!(alpha(1.02), 0.0);
        assert_eq!(alpha(1.1), 0.0);
    }

    #[test]
    fn atlas_uv_corners() {
        let p = atlas_uv(Vec2::ZERO, 0);
        assert!((p - Vec2::new(MARGIN / 2.0, (1.0 - MARGIN) / 2.0)).length() < 1e-6);
        for c in [Vec2::ZERO, Vec2::ONE, Vec2::new(0.5, 0.3)] {
            let q = atlas_uv(c, DENSE_TILE);
            assert!((0.5..=1.0).contains(&q.x) && (0.5..=1.0).contains(&q.y));
        }
    }
}
