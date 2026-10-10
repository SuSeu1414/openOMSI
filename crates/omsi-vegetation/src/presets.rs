//! Parameter presets per archetype.

use crate::classify::Archetype;
use crate::generator::{LeafParams, LevelParams, Shape, TreeParams};

#[allow(clippy::too_many_arguments)]
const fn lv(
    length: f32,
    length_v: f32,
    curve_res: u32,
    curve: f32,
    curve_back: f32,
    curve_v: f32,
    down_angle: f32,
    down_angle_v: f32,
    rotate: f32,
    rotate_v: f32,
    branches: u32,
) -> LevelParams {
    LevelParams {
        length,
        length_v,
        taper: 1.0,
        curve_res,
        curve,
        curve_back,
        curve_v,
        down_angle,
        down_angle_v,
        rotate,
        rotate_v,
        branches,
    }
}

const UNUSED: LevelParams = LevelParams {
    taper: 0.0,
    ..lv(0.0, 0.0, 1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0)
};

/// Typical (archetype, height, width) per preset, for tests.
#[cfg(test)]
pub(crate) const TARGETS: [(Archetype, f32, f32); 8] = [
    (Archetype::BroadleafWide, 15.0, 10.0),
    (Archetype::ConiferConical, 20.0, 6.0),
    (Archetype::BroadleafNarrow, 20.0, 5.0),
    (Archetype::Pine, 18.0, 8.0),
    (Archetype::Willow, 10.0, 10.0),
    (Archetype::Birch, 14.0, 6.0),
    (Archetype::Shrub, 3.0, 3.0),
    (Archetype::Hedge, 2.0, 2.0),
];

/// `None` for archetypes without a generator preset.
pub fn preset(a: Archetype) -> Option<TreeParams> {
    match a {
        Archetype::BroadleafWide => Some(broadleaf_wide()),
        Archetype::ConiferConical => Some(conifer_conical()),
        Archetype::BroadleafNarrow => Some(broadleaf_narrow()),
        Archetype::Pine => Some(pine()),
        Archetype::Willow => Some(willow()),
        Archetype::Birch => Some(birch()),
        Archetype::Shrub => Some(shrub()),
        Archetype::Hedge => Some(hedge()),
        Archetype::None => None,
    }
}

/// Lime / chestnut: decurrent, round crown.
fn broadleaf_wide() -> TreeParams {
    TreeParams {
        shape: Shape::Spherical,
        levels: 3,
        level: [
            lv(1.0, 0.0, 8, 0.0, 0.0, 20.0, 0.0, 0.0, 0.0, 0.0, 0),
            lv(
                0.50, 0.08, 6, -15.0, 0.0, 40.0, 55.0, -20.0, 137.5, 15.0, 36,
            ),
            lv(0.40, 0.10, 4, -10.0, 0.0, 60.0, 45.0, 15.0, 137.5, 25.0, 14),
            UNUSED,
        ],
        base_size: 0.30,
        ratio: 0.018,
        ratio_power: 1.05,
        flare: 0.6,
        attraction_up: 0.6,
        crown_ratio: 0.70,
        prune_peak: 0.5,
        prune_power_low: 0.5,
        prune_power_high: 0.5,
        dead_branches: 0.15,
        trunk_lean: 4.0,
        asymmetry: 0.12,
        trunk_forks: 3,
        fork_height: 0.45,
        fork_angle: 28.0,
        leader_length: 0.85,
        leader_up: 0.6,
        leaves: LeafParams {
            size: 0.8,
            spacing: 0.5,
            start: 0.3,
            angle: 45.0,
            angle_v: 15.0,
            up: 0.6,
        },
    }
}

/// Spruce / fir: excurrent.
fn conifer_conical() -> TreeParams {
    TreeParams {
        shape: Shape::Conical,
        levels: 3,
        level: [
            lv(1.0, 0.0, 10, 0.0, 0.0, 6.0, 0.0, 0.0, 0.0, 0.0, 0),
            lv(
                0.42, 0.05, 5, 15.0, -25.0, 20.0, 80.0, -15.0, 137.5, 10.0, 80,
            ),
            lv(0.35, 0.10, 3, 0.0, 0.0, 30.0, 50.0, 10.0, -1.0, 20.0, 16),
            UNUSED,
        ],
        base_size: 0.12,
        ratio: 0.017,
        ratio_power: 1.0,
        flare: 0.4,
        attraction_up: -0.2,
        crown_ratio: 0.40,
        prune_peak: 0.9,
        prune_power_low: 1.0,
        prune_power_high: 0.5,
        dead_branches: 0.25,
        trunk_lean: 1.5,
        asymmetry: 0.05,
        trunk_forks: 0,
        fork_height: 0.0,
        fork_angle: 0.0,
        leader_length: 1.0,
        leader_up: 0.0,
        leaves: LeafParams {
            size: 0.6,
            spacing: 0.45,
            start: 0.1,
            angle: 60.0,
            angle_v: 15.0,
            up: 0.85,
        },
    }
}

fn broadleaf_narrow() -> TreeParams {
    TreeParams {
        shape: Shape::Cylindrical,
        levels: 3,
        level: [
            lv(1.0, 0.0, 10, 0.0, 0.0, 8.0, 0.0, 0.0, 0.0, 0.0, 0),
            lv(0.35, 0.06, 5, -40.0, 0.0, 20.0, 30.0, 10.0, 137.5, 15.0, 50),
            lv(0.35, 0.1, 3, -10.0, 0.0, 30.0, 30.0, 10.0, 137.5, 30.0, 8),
            UNUSED,
        ],
        base_size: 0.08,
        ratio: 0.016,
        ratio_power: 1.0,
        flare: 0.4,
        attraction_up: 0.8,
        crown_ratio: 0.25,
        prune_peak: 0.6,
        prune_power_low: 0.7,
        prune_power_high: 0.4,
        dead_branches: 0.1,
        trunk_lean: 2.0,
        asymmetry: 0.05,
        trunk_forks: 0,
        fork_height: 0.0,
        fork_angle: 0.0,
        leader_length: 1.0,
        leader_up: 0.0,
        leaves: LeafParams {
            size: 0.6,
            spacing: 0.5,
            start: 0.2,
            angle: 30.0,
            angle_v: 10.0,
            up: 0.3,
        },
    }
}

fn pine() -> TreeParams {
    TreeParams {
        shape: Shape::TendFlame,
        levels: 3,
        level: [
            lv(1.0, 0.0, 10, 0.0, 0.0, 25.0, 0.0, 0.0, 0.0, 0.0, 0),
            lv(0.35, 0.1, 5, -20.0, 0.0, 50.0, 65.0, 20.0, 137.5, 30.0, 28),
            lv(0.45, 0.1, 3, -10.0, 0.0, 40.0, 40.0, 20.0, 137.5, 40.0, 12),
            UNUSED,
        ],
        base_size: 0.55,
        ratio: 0.018,
        ratio_power: 1.0,
        flare: 0.3,
        attraction_up: 0.4,
        crown_ratio: 0.45,
        prune_peak: 0.6,
        prune_power_low: 0.6,
        prune_power_high: 0.5,
        dead_branches: 0.3,
        trunk_lean: 6.0,
        asymmetry: 0.25,
        trunk_forks: 0,
        fork_height: 0.0,
        fork_angle: 0.0,
        leader_length: 1.0,
        leader_up: 0.0,
        leaves: LeafParams {
            size: 0.5,
            spacing: 0.4,
            start: 0.5,
            angle: 40.0,
            angle_v: 15.0,
            up: 0.5,
        },
    }
}

fn willow() -> TreeParams {
    TreeParams {
        shape: Shape::Hemispherical,
        levels: 3,
        level: [
            lv(1.0, 0.0, 8, 0.0, 0.0, 15.0, 0.0, 0.0, 0.0, 0.0, 0),
            lv(0.55, 0.1, 6, 70.0, 0.0, 30.0, 40.0, 10.0, 137.5, 20.0, 28),
            lv(0.6, 0.1, 4, 30.0, 0.0, 20.0, 30.0, 10.0, 137.5, 30.0, 12),
            UNUSED,
        ],
        base_size: 0.15,
        ratio: 0.022,
        ratio_power: 1.1,
        flare: 0.7,
        attraction_up: -2.5,
        crown_ratio: 1.0,
        prune_peak: 0.5,
        prune_power_low: 0.4,
        prune_power_high: 0.3,
        dead_branches: 0.1,
        trunk_lean: 5.0,
        asymmetry: 0.15,
        trunk_forks: 3,
        fork_height: 0.3,
        fork_angle: 30.0,
        leader_length: 0.8,
        leader_up: 0.3,
        leaves: LeafParams {
            size: 0.9,
            spacing: 0.5,
            start: 0.2,
            angle: 15.0,
            angle_v: 10.0,
            up: 0.1,
        },
    }
}

fn birch() -> TreeParams {
    TreeParams {
        shape: Shape::TendFlame,
        levels: 3,
        level: [
            lv(1.0, 0.0, 10, 0.0, 0.0, 15.0, 0.0, 0.0, 0.0, 0.0, 0),
            lv(
                0.4, 0.08, 6, -20.0, 15.0, 30.0, 45.0, -15.0, 137.5, 15.0, 40,
            ),
            lv(0.45, 0.1, 4, 20.0, 0.0, 30.0, 40.0, 15.0, 137.5, 30.0, 14),
            UNUSED,
        ],
        base_size: 0.3,
        ratio: 0.014,
        ratio_power: 1.0,
        flare: 0.3,
        attraction_up: -0.8,
        crown_ratio: 0.5,
        prune_peak: 0.55,
        prune_power_low: 0.6,
        prune_power_high: 0.5,
        dead_branches: 0.15,
        trunk_lean: 5.0,
        asymmetry: 0.15,
        trunk_forks: 0,
        fork_height: 0.0,
        fork_angle: 0.0,
        leader_length: 1.0,
        leader_up: 0.0,
        leaves: LeafParams {
            size: 0.6,
            spacing: 0.5,
            start: 0.3,
            angle: 35.0,
            angle_v: 15.0,
            up: 0.4,
        },
    }
}

fn shrub() -> TreeParams {
    TreeParams {
        shape: Shape::Spherical,
        levels: 3,
        level: [
            lv(1.0, 0.0, 6, 0.0, 0.0, 30.0, 0.0, 0.0, 0.0, 0.0, 0),
            lv(0.45, 0.1, 4, -15.0, 0.0, 40.0, 50.0, 15.0, 137.5, 30.0, 30),
            lv(0.5, 0.1, 3, 0.0, 0.0, 40.0, 45.0, 15.0, 137.5, 30.0, 10),
            UNUSED,
        ],
        base_size: 0.05,
        ratio: 0.02,
        ratio_power: 1.0,
        flare: 0.0,
        attraction_up: 0.3,
        crown_ratio: 1.0,
        prune_peak: 0.55,
        prune_power_low: 0.6,
        prune_power_high: 0.4,
        dead_branches: 0.1,
        trunk_lean: 0.0,
        asymmetry: 0.15,
        trunk_forks: 5,
        fork_height: 0.02,
        fork_angle: 25.0,
        leader_length: 0.85,
        leader_up: 0.3,
        leaves: LeafParams {
            size: 0.35,
            spacing: 0.5,
            start: 0.2,
            angle: 45.0,
            angle_v: 20.0,
            up: 0.5,
        },
    }
}

fn hedge() -> TreeParams {
    TreeParams {
        shape: Shape::Cylindrical,
        levels: 3,
        level: [
            lv(1.0, 0.0, 6, 0.0, 0.0, 30.0, 0.0, 0.0, 0.0, 0.0, 0),
            lv(0.45, 0.1, 4, -15.0, 0.0, 40.0, 60.0, 15.0, 137.5, 30.0, 40),
            lv(0.5, 0.1, 3, 0.0, 0.0, 40.0, 45.0, 15.0, 137.5, 30.0, 10),
            UNUSED,
        ],
        base_size: 0.05,
        ratio: 0.02,
        ratio_power: 1.0,
        flare: 0.0,
        attraction_up: 0.3,
        crown_ratio: 1.0,
        prune_peak: 0.5,
        prune_power_low: 0.1,
        prune_power_high: 0.1,
        dead_branches: 0.1,
        trunk_lean: 0.0,
        asymmetry: 0.15,
        trunk_forks: 6,
        fork_height: 0.02,
        fork_angle: 20.0,
        leader_length: 0.85,
        leader_up: 0.3,
        leaves: LeafParams {
            size: 0.3,
            spacing: 0.45,
            start: 0.1,
            angle: 50.0,
            angle_v: 20.0,
            up: 0.5,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_exist() {
        for (a, _, _) in TARGETS {
            assert!(preset(a).is_some());
        }
        assert!(preset(Archetype::None).is_none());
    }
}
