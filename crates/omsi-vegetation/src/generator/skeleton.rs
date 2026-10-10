//! Deterministic Weber-Penn skeleton: stems as polylines with a radius and a frame per point.
//! Z up, metres, tree base at the origin.

use std::f32::consts::{PI, TAU};

use glam::{Quat, Vec3};

use super::params::{envelope, shape_ratio, Shape, TreeParams};
use crate::seed::{splitmix64, Rng};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub height: f32,
    /// Crown diameter.
    pub width: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stem {
    pub level: u8,
    /// Index into `Skeleton::stems`; `None` only for the trunk.
    pub parent: Option<u32>,
    /// Where on the parent it starts, fraction of the parent's length.
    pub parent_t: f32,
    pub dead: bool,
    /// `curve_res + 1` points, `points[0]` = start.
    pub points: Vec<Vec3>,
    pub radii: Vec<f32>,
    /// `frame * Vec3::Z` = heading of the segment after the point (last point: last segment).
    pub frames: Vec<Quat>,
}

/// `stems[0]` = trunk; every parent index < own index.
#[derive(Clone, Debug, PartialEq)]
pub struct Skeleton {
    pub stems: Vec<Stem>,
    /// Root flare of the trunk; `None` when the preset has no flare.
    pub flare: Option<Flare>,
}

/// Root flare with buttresses, applied to the trunk while meshing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flare {
    /// The preset's `flare`.
    pub amount: f32,
    /// 3..=5.
    pub buttresses: u32,
    /// Radians, [0, TAU).
    pub phase: f32,
    /// Metres of trunk (arc length) the flare covers: H / 8.
    pub zone: f32,
}

pub const MIN_RADIUS: f32 = 0.002;

struct Ctx<'a> {
    p: &'a TreeParams,
    seed: u64,
}

impl Ctx<'_> {
    #[allow(clippy::too_many_arguments)]
    fn grow(
        &self,
        n: usize,
        start: Vec3,
        frame: Quat,
        len: f32,
        radius: f32,
        parent: Option<u32>,
        t: f32,
        idx: usize,
        taper_span: f32,
    ) -> Stem {
        let lp = &self.p.level[n];
        let segs = lp.curve_res.max(1) as usize;
        let seg = len / segs as f32;
        let half = ((segs / 2).max(1)) as f32;
        let mut c = Rng::new(splitmix64(
            self.seed ^ (idx as u64).wrapping_mul(0xD1B5_4A32_D192_ED03),
        ));
        let mut pts = vec![start];
        let mut frames = Vec::with_capacity(segs + 1);
        let mut q = frame;
        for i in 0..segs {
            if i > 0 {
                let mut a = if lp.curve_back == 0.0 {
                    lp.curve / segs as f32
                } else if i < segs.div_ceil(2) {
                    lp.curve / half
                } else {
                    lp.curve_back / half
                };
                a += c.signed() * lp.curve_v / segs as f32;
                q *= Quat::from_rotation_x(a.to_radians());
                if n >= 2 && self.p.attraction_up != 0.0 {
                    q = tropism(q, self.p.attraction_up / segs as f32);
                }
            }
            q = q.normalize();
            frames.push(q);
            let last = *pts.last().unwrap();
            pts.push(last + (q * Vec3::Z) * seg);
        }
        frames.push(*frames.last().unwrap());
        let radii = (0..=segs)
            .map(|k| {
                (radius * (1.0 - lp.taper * (k as f32 / segs as f32) * taper_span)).max(MIN_RADIUS)
            })
            .collect();
        Stem {
            level: n as u8,
            parent,
            parent_t: t,
            dead: false,
            points: pts,
            radii,
            frames,
        }
    }
}

/// Turn the heading of `q` towards +Z (`k` > 0) or away from it (`k` < 0) by `k * declination`.
fn tropism(q: Quat, k: f32) -> Quat {
    let h = q * Vec3::Z;
    let decl = h.z.clamp(-1.0, 1.0).acos();
    let axis = h.cross(Vec3::Z);
    if axis.length() <= 1e-4 {
        return q;
    }
    let ang = k * decl;
    let ang = if ang > 0.0 {
        ang.min(decl)
    } else {
        ang.max(-(PI - decl))
    };
    Quat::from_axis_angle(axis.normalize(), ang) * q
}

fn stem_length(s: &Stem) -> f32 {
    s.points.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Position, frame and radius at fraction `t` of a stem (equal segments).
pub(super) fn sample(s: &Stem, t: f32) -> (Vec3, Quat, f32) {
    let segs = s.points.len() - 1;
    let x = t.clamp(0.0, 1.0) * segs as f32;
    let i = (x.floor() as usize).min(segs - 1);
    let f = x - i as f32;
    (
        s.points[i].lerp(s.points[i + 1], f),
        s.frames[i],
        s.radii[i] + (s.radii[i + 1] - s.radii[i]) * f,
    )
}

pub fn skeleton(p: &TreeParams, target: Target, seed: u64) -> Skeleton {
    let (h, w) = (target.height, target.width);
    let mut r = Rng::new(splitmix64(seed ^ 0x5EED_7EE5));
    let k_len = ((w / h) / p.crown_ratio).clamp(0.5, 2.0);
    let zb = p.base_size * h;
    let lean_dir = r.unit() * TAU;
    let lean = r.unit() * p.trunk_lean;
    let asym_dir = r.unit() * TAU;
    let asym = glam::Vec2::new(asym_dir.cos(), asym_dir.sin()) * p.asymmetry * w / 2.0;

    let inside = |q: Vec3| {
        let rr = (q.z - zb) / (h - zb);
        let e = envelope(rr, p.prune_peak, p.prune_power_low, p.prune_power_high);
        let c = asym * rr.clamp(0.0, 1.0);
        q.z <= h && (q.truncate() - c).length() <= (w / 2.0) * e
    };

    let cx = Ctx { p, seed };
    let l0 = h * p.level[0].length;
    let r0 = l0 * p.ratio;
    let frame0 = Quat::from_rotation_z(lean_dir) * Quat::from_rotation_x(lean.to_radians());
    let forks = if p.trunk_forks >= 2 { p.trunk_forks } else { 0 };
    let mut stems = Vec::new();
    if forks == 0 {
        stems.push(cx.grow(0, Vec3::ZERO, frame0, l0, r0, None, 0.0, 0, 1.0));
    } else {
        let fh = p.fork_height;
        let lt = l0 * fh;
        stems.push(cx.grow(0, Vec3::ZERO, frame0, lt, r0, None, 0.0, 0, fh));
        let rf = r0 * (1.0 - p.level[0].taper * fh);
        let rl = (rf / (forks as f32).sqrt()).max(MIN_RADIUS);
        let tip = *stems[0].points.last().unwrap();
        let ftip = *stems[0].frames.last().unwrap();
        let phi0 = r.unit() * 360.0;
        for j in 0..forks {
            let phi = phi0 + j as f32 * 360.0 / forks as f32 + r.signed() * 20.0;
            let ang_r = p.fork_angle * (0.75 + 0.25 * r.unit());
            let u = 0.92 + 0.08 * r.unit();
            let len = (h - tip.z) / ang_r.to_radians().cos() * u;
            let ang = ang_r.min((0.35 * w / len).clamp(0.0, 1.0).asin().to_degrees());
            let mut len = (h - tip.z) / ang.to_radians().cos() * u;
            let frame = ftip
                * Quat::from_rotation_z(phi.to_radians())
                * Quat::from_rotation_x(ang.to_radians());
            let idx = stems.len();
            let mut s = cx.grow(0, tip, frame, len, rl, Some(0), 1.0, idx, 1.0);
            for _ in 0..3 {
                let top = s.points.iter().map(|q| q.z).fold(f32::MIN, f32::max);
                if top <= h {
                    break;
                }
                len *= (h - tip.z) / (top - tip.z) * 0.999;
                s = cx.grow(0, tip, frame, len, rl, Some(0), 1.0, idx, 1.0);
            }
            stems.push(s);
        }
    }

    let mut lmax = [l0; 4];
    lmax[1] = l0 * p.level[1].length * k_len;
    for n in 2..4 {
        lmax[n] = lmax[n - 1] * p.level[n].length;
    }

    let mut parent = 0;
    while parent < stems.len() {
        let (n, dead, lp_len, rp, z_a, z_b, is_trunk) = {
            let s = &stems[parent];
            (
                s.level as usize + 1,
                s.dead,
                stem_length(s),
                s.radii[0],
                s.points[0].z,
                s.points.last().unwrap().z,
                s.parent.is_none(),
            )
        };
        let zspan = (z_b - z_a).max(1e-3);
        let pi = parent;
        parent += 1;
        if n >= p.levels as usize || dead {
            continue;
        }
        let lp = p.level[n];
        let count = if n == 1 {
            let k = lp.branches as f32 * zspan / h;
            (if is_trunk {
                k
            } else {
                k / (forks.max(1) as f32).sqrt()
            })
            .round() as u32
        } else {
            (lp.branches as f32 * (0.2 + 0.8 * (lp_len / lmax[n - 1]).clamp(0.0, 1.0))).round()
                as u32
        };
        let s0 = if n == 1 {
            ((zb - z_a) / zspan).clamp(0.0, 0.95)
        } else {
            0.0
        };
        let mut rot = r.unit() * 360.0;
        for j in 0..count {
            let t = s0 + (1.0 - s0) * (j as f32 + 0.5) / count as f32;
            rot = if lp.rotate >= 0.0 {
                rot + lp.rotate + r.signed() * lp.rotate_v
            } else {
                rot + 180.0 + r.signed() * lp.rotate_v
            };
            let down = if lp.down_angle_v >= 0.0 {
                lp.down_angle + r.signed() * lp.down_angle_v
            } else {
                lp.down_angle
                    + lp.down_angle_v
                        * (1.0
                            - 2.0
                                * shape_ratio(
                                    Shape::Conical,
                                    (lp_len - t * lp_len) / (lp_len - s0 * lp_len),
                                ))
            };
            let len_f = lp.length + r.signed() * lp.length_v;
            let offset = t * lp_len;
            let (pos, frame_p, rad_p) = sample(&stems[pi], t);
            let mut len = if n == 1 {
                let rr = ((h - pos.z) / (h - zb).max(1e-3)).clamp(0.0, 1.0);
                l0 * len_f * shape_ratio(p.shape, rr) * k_len
            } else {
                len_f * (lp_len - 0.6 * offset)
            };
            let is_dead = n == 1 && {
                let d = r.unit();
                pos.z < zb + 0.3 * (h - zb) && d < p.dead_branches
            };
            if is_dead {
                len *= 0.4;
            }
            let cap = 0.9 * rad_p;
            let rad = if cap < MIN_RADIUS {
                MIN_RADIUS
            } else {
                (rp * (len / lp_len).powf(p.ratio_power)).clamp(MIN_RADIUS, cap)
            };
            let frame = frame_p
                * Quat::from_rotation_z(rot.to_radians())
                * Quat::from_rotation_x(down.to_radians());
            let idx = stems.len();
            let mut found = None;
            for _ in 0..6 {
                let stem = cx.grow(n, pos, frame, len, rad, Some(pi as u32), t, idx, 1.0);
                if stem.points[1..].iter().all(|&q| inside(q)) {
                    found = Some(stem);
                    break;
                }
                len *= 0.75;
            }
            if let Some(mut stem) = found {
                if len >= 0.05 {
                    stem.dead = is_dead;
                    stems.push(stem);
                }
            }
        }
    }
    let mut fr = Rng::new(splitmix64(seed ^ 0xF1A2_E000));
    let flare = (p.flare > 0.0).then(|| Flare {
        amount: p.flare,
        buttresses: 3 + (fr.next_u64() % 3) as u32,
        phase: fr.unit() * TAU,
        zone: h / 8.0,
    });
    Skeleton { stems, flare }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::Archetype;
    use crate::generator::Shape;
    use crate::presets::preset;

    fn both() -> [TreeParams; 2] {
        [
            preset(Archetype::BroadleafWide).unwrap(),
            preset(Archetype::ConiferConical).unwrap(),
        ]
    }

    fn digest(s: &Skeleton) -> u64 {
        let mut d = 0u64;
        let mut f = |x: f32| d = splitmix64(d ^ x.to_bits() as u64);
        for st in &s.stems {
            st.points
                .iter()
                .for_each(|v| v.to_array().into_iter().for_each(&mut f));
            st.radii.iter().copied().for_each(&mut f);
            st.frames
                .iter()
                .for_each(|q| q.to_array().into_iter().for_each(&mut f));
        }
        d
    }

    const T: Target = Target {
        height: 15.0,
        width: 10.0,
    };

    #[test]
    fn same_seed_same_skeleton() {
        for p in both() {
            let (a, b) = (skeleton(&p, T, 42), skeleton(&p, T, 42));
            assert_eq!(a, b);
            assert_eq!(digest(&a), digest(&b));
        }
    }

    #[test]
    fn other_seed_other_skeleton() {
        for p in both() {
            assert_ne!(digest(&skeleton(&p, T, 1)), digest(&skeleton(&p, T, 2)));
        }
    }

    #[test]
    fn fits_the_target_box() {
        for p in both() {
            for (height, width) in [(15.0, 10.0), (8.0, 3.0), (25.0, 20.0)] {
                for seed in 0..8 {
                    let sk = skeleton(&p, Target { height, width }, seed);
                    for s in sk.stems.iter().filter(|s| s.level >= 1) {
                        for v in &s.points {
                            assert!(v.z <= height + 1e-3, "z {} > {height}", v.z);
                            assert!(
                                v.truncate().length()
                                    <= width / 2.0 + width * p.asymmetry / 2.0 + 1e-3
                            );
                        }
                    }
                    let l0 = || sk.stems.iter().filter(|s| s.level == 0);
                    for v in l0().flat_map(|s| &s.points) {
                        assert!(v.z <= height + 1e-3, "level-0 z {} > {height}", v.z);
                    }
                    let top = l0()
                        .flat_map(|s| &s.points)
                        .map(|v| v.z)
                        .fold(f32::MIN, f32::max);
                    assert!((0.9 * height..=1.0001 * height).contains(&top), "top {top}");
                }
            }
        }
    }

    #[test]
    fn structure_is_sane() {
        for p in both() {
            let sk = skeleton(&p, T, 7);
            for (i, s) in sk.stems.iter().enumerate() {
                let n = p.level[s.level as usize].curve_res as usize + 1;
                assert_eq!((s.points.len(), s.radii.len(), s.frames.len()), (n, n, n));
                assert!(s.radii.windows(2).all(|w| w[1] <= w[0]));
                assert!(s.radii.iter().all(|&r| r >= MIN_RADIUS));
                match s.parent {
                    None => assert_eq!(i, 0),
                    Some(pi) => {
                        if s.level == 0 {
                            assert_eq!(pi, 0);
                        }
                        assert!((pi as usize) < i);
                        let (_, _, rad) = sample(&sk.stems[pi as usize], s.parent_t);
                        assert!(s.radii[0] <= rad + 1e-6);
                    }
                }
            }
        }
        let count = |sk: &Skeleton, l| sk.stems.iter().filter(|s| s.level == l).count();
        let [b, c] = both();
        let sb = skeleton(&b, T, 7);
        assert!((100..=2000).contains(&sb.stems.len()) && count(&sb, 1) >= 15);
        assert!(
            count(
                &skeleton(
                    &c,
                    Target {
                        height: 20.0,
                        width: 6.0
                    },
                    7
                ),
                1
            ) >= 30
        );
    }

    #[test]
    fn forks_make_leaders() {
        let [b, c] = both();
        for seed in 0..8 {
            let sk = skeleton(&b, T, seed);
            let l0: Vec<_> = sk.stems.iter().filter(|s| s.level == 0).collect();
            assert_eq!(l0.len(), 4);
            let t = &sk.stems[0];
            for l in &l0[1..] {
                assert_eq!(l.points[0], *t.points.last().unwrap());
                assert!(l.radii[0] < t.radii.last().unwrap() + 1e-6);
            }
            assert!(*t.radii.last().unwrap() >= 0.4 * t.radii[0]);
            let sc = skeleton(&c, T, seed);
            assert_eq!(sc.stems.iter().filter(|s| s.level == 0).count(), 1);
        }
    }

    #[test]
    fn leaders_stay_narrow() {
        let [b, _] = both();
        let (height, width) = (8.0, 3.0);
        for seed in 0..8 {
            let sk = skeleton(&b, Target { height, width }, seed);
            let lim = 0.35 * width
                + height * b.trunk_lean.to_radians().sin()
                + (height - b.fork_height * height) * b.level[0].curve_v.to_radians().sin()
                + 1e-3;
            for v in sk
                .stems
                .iter()
                .filter(|s| s.level == 0)
                .flat_map(|s| &s.points)
            {
                assert!(
                    v.truncate().length() <= lim,
                    "seed {seed} {} > {lim} tip {:?}",
                    v.truncate().length(),
                    sk.stems[0].points.last()
                );
            }
        }
    }

    #[test]
    fn tropism_turns_up() {
        let q = Quat::from_rotation_x(60f32.to_radians());
        let before = (q * Vec3::Z).z;
        assert!((tropism(q, 0.5) * Vec3::Z).z > before);
        assert!((tropism(q, -0.5) * Vec3::Z).z < before);
    }

    #[test]
    fn envelope_and_shapes() {
        assert_eq!(envelope(0.5, 0.5, 0.5, 0.5), 1.0);
        assert_eq!(envelope(-0.1, 0.5, 0.5, 0.5), 0.0);
        assert_eq!(envelope(1.1, 0.5, 0.5, 0.5), 0.0);
        assert_eq!(shape_ratio(Shape::Conical, 1.0), 1.0);
        assert_eq!(shape_ratio(Shape::Flame, 0.7), 1.0);
    }

    #[test]
    fn skeleton_stream_unchanged() {
        let [b, c] = both();
        let tc = Target {
            height: 20.0,
            width: 6.0,
        };
        for (p, t, want) in [(&b, T, [4, 38, 302, 0]), (&c, tc, [1, 62, 363, 0])] {
            let sk = skeleton(p, t, 42);
            let mut bare = sk.clone();
            bare.flare = None;
            assert_eq!(digest(&sk), digest(&bare));
            let got: Vec<usize> = (0..4)
                .map(|l| sk.stems.iter().filter(|s| s.level == l).count())
                .collect();
            assert_eq!(got, want);
        }
    }
}
