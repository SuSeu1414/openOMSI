//! Rings along a stem: the stem's own points plus extra rings, and the radius modifiers
//! (root flare with buttresses, branch collars) applied while meshing.

use glam::{Quat, Vec3};

use super::skeleton::{Flare, Stem};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Ring {
    /// Arc length from `points[0]`.
    pub s: f32,
    pub pos: Vec3,
    pub frame: Quat,
    pub radius: f32,
}

/// The frame of the ring at point `i`: halfway between the neighbouring segment frames.
fn ring_frame(st: &Stem, i: usize) -> Quat {
    if i == 0 || i == st.points.len() - 1 {
        st.frames[i]
    } else {
        st.frames[i - 1].slerp(st.frames[i], 0.5)
    }
}

/// The stem's own points as rings, plus rings at the extra arc lengths (sorted, deduplicated
/// within 1e-4 m, clamped to `[-radii[0], length]`); `s < 0` extrapolates along `-frames[0] * Z`.
pub(crate) fn resample(stem: &Stem, extra: &[f32]) -> Vec<Ring> {
    let m = stem.points.len();
    let mut at = vec![0.0f32];
    for w in stem.points.windows(2) {
        at.push(at.last().unwrap() + w[0].distance(w[1]));
    }
    let len = at[m - 1];
    let mut rings: Vec<Ring> = (0..m)
        .map(|i| Ring {
            s: at[i],
            pos: stem.points[i],
            frame: ring_frame(stem, i),
            radius: stem.radii[i],
        })
        .collect();
    for &e in extra {
        let s = e.clamp(-stem.radii[0], len);
        if rings.iter().any(|r| (r.s - s).abs() < 1e-4) {
            continue;
        }
        rings.push(if s < 0.0 {
            Ring {
                s,
                pos: stem.points[0] + (stem.frames[0] * Vec3::Z) * s,
                frame: stem.frames[0],
                radius: stem.radii[0],
            }
        } else {
            let i = (0..m - 1).rfind(|&i| at[i] <= s).unwrap_or(0);
            let f = ((s - at[i]) / (at[i + 1] - at[i]).max(1e-9)).clamp(0.0, 1.0);
            Ring {
                s,
                pos: stem.points[i].lerp(stem.points[i + 1], f),
                frame: ring_frame(stem, i).slerp(ring_frame(stem, i + 1), f),
                radius: stem.radii[i] + (stem.radii[i + 1] - stem.radii[i]) * f,
            }
        });
    }
    rings.sort_by(|a, b| a.s.total_cmp(&b.s));
    rings
}

/// Radius of the trunk ring `r` at angle `th`: root flare plus buttress lobes.
pub(crate) fn flare_radius(f: &Flare, r: &Ring, th: f32) -> f32 {
    let s = r.s.max(0.0);
    let fl = if s < f.zone {
        1.0 + f.amount * (100f32.powf(1.0 - s / f.zone) - 1.0) / 100.0
    } else {
        1.0
    };
    let w = if s < f.zone * 0.7 {
        (1.0 - s / (f.zone * 0.7)).powi(2)
    } else {
        0.0
    };
    let lobe = (f.buttresses as f32 * (th - f.phase))
        .cos()
        .max(0.0)
        .powi(3);
    r.radius * fl * (1.0 + 0.5 * f.amount * w * lobe)
}

/// Radius of a branch ring that blends into its parent (radius `rp` there); `r0` = base radius.
pub(crate) fn collar_radius(r: &Ring, rp: f32, r0: f32) -> f32 {
    let k = if r.s < rp {
        1.8
    } else {
        let q = ((r.s - rp) / (3.0 * r0)).clamp(0.0, 1.0);
        1.0 + 0.8 * (1.0 - q) * (1.0 - q)
    };
    (r.radius * k).min(0.95 * rp)
}
