//! Periodic value noise, fbm and Worley noise. `p` is in lattice units; `period` is per axis in
//! lattice cells (>= 1). Sampling texel (x, y) at `texel_p` makes an N-texel texture tile exactly.

use crate::seed::splitmix64;
use glam::Vec2;

/// Distances to the two nearest feature points (in cell units) and an id of the nearest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub f1: f32,
    pub f2: f32,
    pub id: f32,
}

/// Lattice hash in [0, 1); `salt` picks an independent stream.
fn hash(ix: i32, iy: i32, period: [u32; 2], seed: u64, salt: u64) -> f32 {
    let wx = ix.rem_euclid(period[0].max(1) as i32) as u64;
    let wy = iy.rem_euclid(period[1].max(1) as i32) as u64;
    let h = splitmix64(seed ^ salt ^ (wx << 32) ^ wy ^ 0x9e37_79b9);
    (h >> 40) as f32 / 16_777_216.0
}

/// Value noise in [0, 1].
pub fn value(p: Vec2, period: [u32; 2], seed: u64) -> f32 {
    let i = p.floor();
    let f = p - i;
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let (ix, iy) = (i.x as i32, i.y as i32);
    let h = |dx, dy| hash(ix + dx, iy + dy, period, seed, 0);
    let top = h(0, 0) + (h(1, 0) - h(0, 0)) * u.x;
    let bot = h(0, 1) + (h(1, 1) - h(0, 1)) * u.x;
    top + (bot - top) * u.y
}

/// Fractal sum of `value` octaves (1..=10), each tiling at the same place; in [0, 1].
pub fn fbm(p: Vec2, period: [u32; 2], octaves: u32, seed: u64) -> f32 {
    let (mut sum, mut norm, mut amp) = (0.0, 0.0, 1.0);
    for k in 0..octaves.clamp(1, 10) {
        let s = (1u32 << k) as f32;
        let per = [period[0] << k, period[1] << k];
        sum += amp * value(p * s, per, seed.wrapping_add(k as u64));
        norm += amp;
        amp *= 0.5;
    }
    sum / norm
}

/// Worley (cellular) noise.
pub fn worley(p: Vec2, period: [u32; 2], seed: u64) -> Cell {
    let c = p.floor();
    let (cx, cy) = (c.x as i32, c.y as i32);
    let (mut f1, mut f2, mut id) = (f32::MAX, f32::MAX, 0.0);
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (nx, ny) = (cx + dx, cy + dy);
            let jx = hash(nx, ny, period, seed, 0x5851_f42d_4c95_7f2d);
            let jy = hash(nx, ny, period, seed, 0x1405_7b7e_f767_814f);
            let d = (p - Vec2::new(nx as f32 + jx, ny as f32 + jy)).length();
            if d < f1 {
                f2 = f1;
                f1 = d;
                id = hash(nx, ny, period, seed, 0x2545_f491_4f6c_dd1d);
            } else if d < f2 {
                f2 = d;
            }
        }
    }
    Cell { f1, f2, id }
}

/// The lattice point of texel (x, y) for a texture of `size` texels.
pub fn texel_p(x: u32, y: u32, size: u32, period: [u32; 2]) -> Vec2 {
    let n = size as f32;
    Vec2::new(
        (x as f32 + 0.5) / n * period[0] as f32,
        (y as f32 + 0.5) / n * period[1] as f32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed::Rng;

    fn rand_p(r: &mut Rng) -> Vec2 {
        Vec2::new(r.unit() * 4.0, r.unit() * 7.0)
    }

    #[test]
    fn value_fbm_worley_tile() {
        let mut r = Rng::new(1);
        let per = [4, 7];
        let off = Vec2::new(4.0, 7.0);
        for _ in 0..50 {
            let p = rand_p(&mut r);
            assert!((value(p, per, 3) - value(p + off, per, 3)).abs() < 1e-5);
            assert!((fbm(p, per, 5, 3) - fbm(p + off, per, 5, 3)).abs() < 1e-5);
            let (a, b) = (worley(p, per, 3), worley(p + off, per, 3));
            assert!((a.f1 - b.f1).abs() < 1e-5 && (a.f2 - b.f2).abs() < 1e-5);
            assert_eq!(a.id, b.id);
        }
    }

    #[test]
    fn noise_in_range_and_deterministic() {
        let mut r = Rng::new(2);
        let mut differs = false;
        for _ in 0..1000 {
            let p = rand_p(&mut r) * 3.0;
            let (v, f) = (value(p, [4, 7], 9), fbm(p, [4, 7], 4, 9));
            assert!((0.0..=1.0).contains(&v) && (0.0..=1.0).contains(&f));
            assert_eq!(v, value(p, [4, 7], 9));
            assert_eq!(f, fbm(p, [4, 7], 4, 9));
            differs |= v != value(p, [4, 7], 10);
        }
        assert!(differs);
        let mut sum = 0.0;
        for y in 0..64 {
            for x in 0..64 {
                sum += fbm(texel_p(x, y, 64, [4, 4]), [4, 4], 4, 5);
            }
        }
        let mean = sum / 4096.0;
        assert!((0.35..=0.65).contains(&mean), "{mean}");
    }

    #[test]
    fn worley_f1_le_f2() {
        let mut r = Rng::new(3);
        for _ in 0..500 {
            let c = worley(rand_p(&mut r), [4, 7], 1);
            assert!(c.f1 <= c.f2 && c.f1 < 1.5);
        }
    }
}
