//! Weber-Penn parameter types and the crown shape functions.

use std::f32::consts::PI;

/// Weber-Penn crown shapes (paper numbering 0-7).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Conical,
    Spherical,
    Hemispherical,
    Cylindrical,
    TaperedCylindrical,
    Flame,
    InverseConical,
    TendFlame,
}

/// Relative branch length at relative height `r` (clamped to [0, 1]).
pub fn shape_ratio(shape: Shape, r: f32) -> f32 {
    let r = r.clamp(0.0, 1.0);
    match shape {
        Shape::Conical => 0.2 + 0.8 * r,
        Shape::Spherical => 0.2 + 0.8 * (PI * r).sin(),
        Shape::Hemispherical => 0.2 + 0.8 * (0.5 * PI * r).sin(),
        Shape::Cylindrical => 1.0,
        Shape::TaperedCylindrical => 0.5 + 0.5 * r,
        Shape::Flame => {
            if r <= 0.7 {
                r / 0.7
            } else {
                (1.0 - r) / 0.3
            }
        }
        Shape::InverseConical => 1.0 - 0.8 * r,
        Shape::TendFlame => {
            if r <= 0.7 {
                0.5 + 0.5 * r / 0.7
            } else {
                0.5 + 0.5 * (1.0 - r) / 0.3
            }
        }
    }
}

/// Pruning envelope (paper's shape 8): 0 outside r in [0, 1].
pub fn envelope(r: f32, peak: f32, power_low: f32, power_high: f32) -> f32 {
    if !(0.0..=1.0).contains(&r) {
        return 0.0;
    }
    let m = 1.0 - peak;
    if r < m {
        (r / m).powf(power_high)
    } else if m >= 1.0 {
        1.0
    } else {
        ((1.0 - r) / (1.0 - m)).powf(power_low)
    }
}

/// One stem level (paper parameters nX). Angles in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelParams {
    /// Fraction of the parent (level 0: of the height).
    pub length: f32,
    pub length_v: f32,
    /// 0..=1.
    pub taper: f32,
    /// Segments per stem, >= 1.
    pub curve_res: u32,
    pub curve: f32,
    pub curve_back: f32,
    pub curve_v: f32,
    pub down_angle: f32,
    pub down_angle_v: f32,
    /// Negative: opposite branches (alternating 180 deg).
    pub rotate: f32,
    pub rotate_v: f32,
    /// Max stems of THIS level on each parent (ignored for level 0).
    pub branches: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeParams {
    pub shape: Shape,
    /// Stem levels used, 1..=4 (`level[0..levels]`).
    pub levels: u32,
    pub level: [LevelParams; 4],
    /// Fraction of the height without branches.
    pub base_size: f32,
    /// Trunk radius / trunk length.
    pub ratio: f32,
    pub ratio_power: f32,
    /// Stored only; used by the mesher.
    pub flare: f32,
    /// Tropism for levels >= 2; negative = droop.
    pub attraction_up: f32,
    /// The preset's natural crown width / height.
    pub crown_ratio: f32,
    pub prune_peak: f32,
    pub prune_power_low: f32,
    pub prune_power_high: f32,
    /// Probability that a low level-1 branch is dead.
    pub dead_branches: f32,
    /// Degrees, max.
    pub trunk_lean: f32,
    /// 0..1, crown centre shift as a fraction of the half-width.
    pub asymmetry: f32,
    /// Number of co-dominant leaders the trunk splits into; 0 or 1 = no fork.
    pub trunk_forks: u32,
    /// Fraction of the height where the trunk forks (0..1).
    pub fork_height: f32,
    /// Degrees from the trunk axis, max.
    pub fork_angle: f32,
    /// Leader length as a fraction of the straight run to the top.
    pub leader_length: f32,
    /// Tropism of fork leaders (level 0 with a parent), like `attraction_up`.
    pub leader_up: f32,
    pub leaves: LeafParams,
}

/// Twig-cluster cards on the outermost stems.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeafParams {
    /// Card height (base to tip) in m; the card is square before the hull crop.
    pub size: f32,
    /// Distance between cards along a twig, as a fraction of `size`.
    pub spacing: f32,
    /// Fraction of the twig length where cards start.
    pub start: f32,
    /// Degrees between the twig heading and the card axis.
    pub angle: f32,
    pub angle_v: f32,
    /// 0..1: 0 = card faces out of the crown, 1 = card faces up.
    pub up: f32,
}
