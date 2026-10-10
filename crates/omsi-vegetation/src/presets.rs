//! Parameter presets per archetype.

use crate::classify::Archetype;
use crate::generator::{LevelParams, Shape, TreeParams};

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

/// `None` for archetypes without a generator preset.
pub fn preset(a: Archetype) -> Option<TreeParams> {
    match a {
        // Lime / chestnut: decurrent, round crown.
        Archetype::BroadleafWide => Some(TreeParams {
            shape: Shape::Spherical,
            levels: 3,
            level: [
                lv(1.0, 0.0, 8, 0.0, 0.0, 20.0, 0.0, 0.0, 0.0, 0.0, 0),
                lv(
                    0.50, 0.08, 6, -30.0, 0.0, 40.0, 45.0, -20.0, 137.5, 15.0, 36,
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
        }),
        // Spruce / fir: excurrent.
        Archetype::ConiferConical => Some(TreeParams {
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
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_exist() {
        assert!(preset(Archetype::BroadleafWide).is_some());
        assert!(preset(Archetype::ConiferConical).is_some());
        for a in [
            Archetype::BroadleafNarrow,
            Archetype::Pine,
            Archetype::Willow,
            Archetype::Birch,
            Archetype::Shrub,
            Archetype::Hedge,
            Archetype::None,
        ] {
            assert!(preset(a).is_none());
        }
    }
}
