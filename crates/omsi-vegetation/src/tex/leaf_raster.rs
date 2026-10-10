//! Raster helpers shared by the leaf and needle atlases: float tile layers with `over`
//! compositing, tapered stem caps and small colour and distance helpers.

use super::leaves::{atlas_uv, MARGIN, TILES};
use crate::generator::leaves::HULL;
use glam::Vec2;

pub(super) fn lin(c: [u8; 3]) -> [f32; 3] {
    c.map(|v| (v as f32 / 255.0).powf(2.2))
}

pub(super) fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(super) fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// Signed distance to the nearest hull edge line (card units): >= 0 inside, exact inside,
/// a lower bound of the distance outside.
pub(super) fn hull_dist(p: Vec2) -> f32 {
    let n = HULL.len();
    (0..n)
        .map(|i| {
            let (a, b) = (HULL[i], HULL[(i + 1) % n]);
            let e = b - a;
            e.perp_dot(p - a) / e.length()
        })
        .fold(f32::MAX, f32::min)
}

/// One painted texel.
pub(super) struct Texel {
    pub(super) col: [f32; 3],
    pub(super) h: f32,
    pub(super) rough: f32,
    pub(super) trans: f32,
}

pub(super) const SUB: usize = 4;

/// Per-tile float layers, premultiplied by coverage (`over` compositing).
pub(super) struct Layer {
    pub(super) t: usize,
    pub(super) a: Vec<f32>,
    pub(super) col: Vec<[f32; 3]>,
    pub(super) h: Vec<f32>,
    pub(super) rough: Vec<f32>,
    pub(super) trans: Vec<f32>,
}

impl Layer {
    pub(super) fn new(t: usize) -> Layer {
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
    pub(super) fn card_at(&self, x: f32, y: f32) -> Vec2 {
        let k = 1.0 - 2.0 * MARGIN;
        let t = self.t as f32;
        Vec2::new((x / t - MARGIN) / k, 1.0 - (y / t - MARGIN) / k)
    }

    pub(super) fn px_of(&self, c: Vec2) -> Vec2 {
        atlas_uv(c, 0) * TILES as f32 * self.t as f32
    }

    /// Coverage by 4x4 subsamples of `inside` per pixel of the card-space box `lo..hi`.
    pub(super) fn paint(
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
pub(super) struct Cap {
    pub(super) a: Vec2,
    pub(super) b: Vec2,
    pub(super) ra: f32,
    pub(super) rb: f32,
}

impl Cap {
    /// Distance to the axis over the local radius (<= 1 inside).
    pub(super) fn rel(&self, p: Vec2) -> f32 {
        let ab = self.b - self.a;
        let t = ((p - self.a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
        (p - (self.a + ab * t)).length() / (self.ra + (self.rb - self.ra) * t).max(1e-6)
    }

    pub(super) fn bbox(&self) -> (Vec2, Vec2) {
        let r = Vec2::splat(self.ra.max(self.rb));
        (self.a.min(self.b) - r, self.a.max(self.b) + r)
    }

    pub(super) fn draw(
        &self,
        l: &mut Layer,
        col: [f32; 3],
        h_axis: f32,
        h_edge: f32,
        rough: f32,
        tr: f32,
    ) {
        l.paint(self.bbox(), &|p| self.rel(p) <= 1.0, &|p| Texel {
            col,
            h: h_axis + (h_edge - h_axis) * self.rel(p).min(1.0),
            rough,
            trans: tr,
        });
    }
}

pub(super) fn rot(v: Vec2, a: f32) -> Vec2 {
    Vec2::from_angle(a).rotate(v)
}
