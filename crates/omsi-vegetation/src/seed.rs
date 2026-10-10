//! Deterministic per-instance seed and the variation derived from it.
//!
//! The seed depends only on the object id, the tile and the position on a 0.1 m grid. It must
//! never depend on the clock, thread order or hash-map order, so every LAN player sees the
//! same tree in the same place.

use std::f32::consts::TAU;

/// The full 64-bit splitmix64 finaliser.
pub fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Position on a 0.1 m grid (NaN -> 0).
fn q(v: f64) -> u64 {
    (v * 10.0).round() as i64 as u64
}

/// The seed of one placed object: stable across runs, machines and players.
pub fn seed(object_id: i64, tile: (i32, i32), pos: [f64; 3]) -> u64 {
    let tile = ((tile.0 as u32 as u64) << 32) | (tile.1 as u32 as u64);
    let mut h = splitmix64(object_id as u64 ^ tile.rotate_left(17));
    for c in pos {
        h = splitmix64(h ^ q(c));
    }
    h
}

/// A number in [0, 1) from a seed and a stream number.
pub fn unit(seed: u64, stream: u64) -> f32 {
    (splitmix64(seed ^ stream.wrapping_mul(0x9E37_79B9_7F4A_7C15)) >> 40) as f32 / 16_777_216.0
}

/// The small differences between two trees of one kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Variation {
    pub variant: u32,
    pub yaw: f32,
    pub scale: f32,
    pub hue_deg: f32,
    pub value: f32,
    pub health: f32,
}

/// `variants`: how many model variants the archetype has (0 = none).
pub fn variation(seed: u64, variants: u32) -> Variation {
    Variation {
        variant: if variants == 0 { 0 } else { ((unit(seed, 1) as f64 * variants as f64) as u32).min(variants - 1) },
        yaw: unit(seed, 2) * TAU,
        scale: 0.94 + 0.12 * unit(seed, 3),
        hue_deg: -4.0 + 8.0 * unit(seed, 4),
        value: 0.92 + 0.16 * unit(seed, 5),
        health: 0.75 + 0.25 * unit(seed, 6),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN_A: u64 = 15901346372513191518;
    const GOLDEN_B: u64 = 7079085327263212397;
    const P: [f64; 3] = [295.81, 213.63, 0.0];

    #[test]
    fn same_cell_same_seed() {
        assert_eq!(seed(5716, (0, 0), P), seed(5716, (0, 0), [295.84, 213.61, 0.04]));
    }

    #[test]
    fn changes_with_inputs() {
        let s = seed(5716, (0, 0), P);
        assert_ne!(s, seed(5717, (0, 0), P));
        assert_ne!(s, seed(5716, (1, 0), P));
        assert_ne!(s, seed(5716, (0, 1), P));
        assert_ne!(s, seed(5716, (0, 0), [295.91, 213.63, 0.0]));
    }

    #[test]
    fn golden() {
        assert_eq!(seed(5716, (0, 0), P), GOLDEN_A);
        assert_eq!(seed(1, (-3, 7), [-0.05, 0.0, 1000.0]), GOLDEN_B);
    }

    #[test]
    fn variation_ranges() {
        for i in 0..10_000u64 {
            let v = variation(splitmix64(i), 5);
            assert!(v.variant < 5);
            assert!((0.0..TAU).contains(&v.yaw));
            assert!((0.94..=1.06).contains(&v.scale));
            assert!((-4.0..=4.0).contains(&v.hue_deg));
            assert!((0.92..=1.08).contains(&v.value));
            assert!((0.75..=1.0).contains(&v.health));
        }
        assert_eq!(variation(1, 0).variant, 0);
    }

    #[test]
    fn variant_histogram() {
        let mut h = [0u32; 8];
        for i in 0..80_000u64 {
            h[variation(splitmix64(i), 8).variant as usize] += 1;
        }
        for c in h {
            assert!((9_400..=10_600).contains(&c), "{h:?}");
        }
    }

    #[test]
    fn unit_range() {
        for i in 0..10_000u64 {
            let u = unit(i, i % 7);
            assert!((0.0..1.0).contains(&u));
        }
    }
}
