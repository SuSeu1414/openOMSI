//! Seasonal leaf palettes as data, plus the CPU reference of the recolour the shader mirrors:
//! one summer atlas per archetype is tinted per card from a gradient (by branch, card and
//! height), with a share of cards shown per season and a lighter, greyer underside.

use super::leaf_raster::{lin, mix3};
use super::leaves::leaf_ref;
use super::ops::unorm8;
use crate::classify::Archetype;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

pub const SEASONS: [Season; 4] = [
    Season::Spring,
    Season::Summer,
    Season::Autumn,
    Season::Winter,
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LeafPalette {
    /// Gradient stops (sRGB) at positions `at` (increasing, at[0] = 0, at[3] = 1).
    pub colours: [[u8; 3]; 4],
    pub at: [f32; 4],
    /// Share of cards shown, 0..1 (0 = bare).
    pub density: f32,
    /// Underside of the leaf: lift toward white and loss of saturation, 0..1.
    pub under_lift: f32,
    pub under_desat: f32,
}

type Stops = [[u8; 3]; 4];

/// Spring stops, autumn stops, densities (spring, summer, autumn), underside (lift, desat).
type Deciduous = (Stops, Stops, [f32; 3], (f32, f32));

fn lum(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn scale(c: [u8; 3], k: f32) -> [u8; 3] {
    c.map(|v| (v as f32 * k).round().clamp(0.0, 255.0) as u8)
}

fn summer_stops(r: [u8; 3]) -> Stops {
    let y = [
        (r[0] as f32 * 1.1).round().min(255.0) as u8,
        (r[1] as f32 * 1.05).round().min(255.0) as u8,
        (r[2] as f32 * 0.8).round() as u8,
    ];
    [scale(r, 0.9), r, y, scale(r, 0.8)]
}

fn deciduous(a: Archetype) -> Option<Deciduous> {
    use Archetype::*;
    Some(match a {
        BroadleafWide => (
            [
                [110, 150, 50],
                [125, 160, 55],
                [140, 165, 60],
                [100, 140, 45],
            ],
            [[96, 112, 40], [170, 150, 40], [200, 150, 40], [140, 90, 40]],
            [0.8, 1.0, 0.85],
            (0.12, 0.35),
        ),
        BroadleafNarrow => (
            [
                [115, 150, 50],
                [130, 160, 55],
                [140, 165, 60],
                [105, 140, 45],
            ],
            [
                [110, 120, 40],
                [190, 170, 50],
                [210, 175, 60],
                [150, 110, 50],
            ],
            [0.8, 1.0, 0.85],
            (0.22, 0.45),
        ),
        Birch => (
            [
                [125, 165, 55],
                [140, 175, 60],
                [150, 175, 65],
                [115, 155, 50],
            ],
            [
                [120, 130, 45],
                [200, 180, 50],
                [220, 190, 70],
                [170, 130, 60],
            ],
            [0.8, 1.0, 0.8],
            (0.12, 0.35),
        ),
        Willow => (
            [
                [125, 155, 65],
                [135, 160, 70],
                [145, 165, 75],
                [115, 145, 60],
            ],
            [
                [110, 125, 55],
                [160, 160, 60],
                [190, 170, 70],
                [150, 130, 70],
            ],
            [0.85, 1.0, 0.85],
            (0.25, 0.50),
        ),
        Shrub => (
            [
                [105, 140, 48],
                [120, 150, 52],
                [135, 155, 58],
                [95, 130, 44],
            ],
            [[100, 100, 40], [180, 110, 40], [170, 60, 35], [110, 50, 35]],
            [0.8, 1.0, 0.8],
            (0.12, 0.35),
        ),
        Hedge => (
            [[95, 135, 45], [110, 145, 50], [120, 150, 55], [85, 125, 42]],
            [[90, 100, 40], [170, 130, 50], [160, 100, 45], [130, 80, 40]],
            [0.85, 1.0, 0.9],
            (0.12, 0.35),
        ),
        _ => return Option::None,
    })
}

/// `None` for `Archetype::None`.
pub fn palette(a: Archetype, s: Season) -> Option<LeafPalette> {
    let summer = summer_stops(leaf_ref(a)?);
    let at = [0.0, 0.35, 0.7, 1.0];
    let Some((spring, autumn, d, (under_lift, under_desat))) = deciduous(a) else {
        // Evergreen conifers: the same needles all year, a little darker in winter.
        let colours = if s == Season::Winter {
            summer.map(|c| scale(c, 0.85))
        } else {
            summer
        };
        return Some(LeafPalette {
            colours,
            at,
            density: 1.0,
            under_lift: 0.05,
            under_desat: 0.15,
        });
    };
    let (colours, density) = match s {
        Season::Spring => (spring, d[0]),
        Season::Summer => (summer, d[1]),
        Season::Autumn => (autumn, d[2]),
        // Dry hedge leaves stay on half the cards; the other deciduous trees are bare.
        Season::Winter if a == Archetype::Hedge => (
            [[150, 110, 65], [140, 100, 60], [125, 90, 55], [110, 80, 50]],
            0.5,
        ),
        Season::Winter => (summer, 0.0),
    };
    Some(LeafPalette {
        colours,
        at,
        density,
        under_lift,
        under_desat,
    })
}

/// Position on the gradient for one card: branch-dominated (whole branches turn together),
/// some per-card noise, the crown top a step ahead.
pub fn gradient_key(rand: f32, phase: f32, height01: f32) -> f32 {
    (0.6 * phase + 0.25 * rand + 0.3 * (height01 - 0.5) + 0.075).clamp(0.0, 1.0)
}

/// Target colour (linear RGB) of a card.
pub fn leaf_colour(p: &LeafPalette, key: f32) -> [f32; 3] {
    let i = (0..3).find(|&i| key <= p.at[i + 1]).unwrap_or(2);
    let t = ((key - p.at[i]) / (p.at[i + 1] - p.at[i])).clamp(0.0, 1.0);
    mix3(lin(p.colours[i]), lin(p.colours[i + 1]), t)
}

/// Whether a card with this rand is drawn in this season.
pub fn card_visible(p: &LeafPalette, rand: f32) -> bool {
    rand < p.density
}

/// CPU reference of the shader recolour for one texel: albedo (sRGB) of the summer atlas,
/// the archetype's `leaf_ref` (sRGB), the card's target (linear) and the translucency byte.
/// Translucency 0 (twig wood) returns the albedo unchanged.
pub fn recolour(albedo: [u8; 3], reference: [u8; 3], target: [f32; 3], trans: u8) -> [u8; 3] {
    if trans == 0 {
        return albedo;
    }
    let k = lum(lin(albedo)) / lum(lin(reference)).max(1e-4);
    target.map(|t| unorm8((t * k).min(1.0).powf(1.0 / 2.2)))
}

/// Underside colour (linear) from the top colour (linear).
pub fn underside(top: [f32; 3], p: &LeafPalette) -> [f32; 3] {
    let l = lum(top);
    let c = mix3(top, [l; 3], p.under_desat);
    mix3(c, [1.0; 3], p.under_lift * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Archetype::*;

    const ALL: [Archetype; 8] = [
        BroadleafWide,
        BroadleafNarrow,
        ConiferConical,
        Pine,
        Willow,
        Birch,
        Shrub,
        Hedge,
    ];

    fn pal(a: Archetype, s: Season) -> LeafPalette {
        palette(a, s).unwrap()
    }

    #[test]
    fn palettes_exist() {
        for a in ALL {
            for s in SEASONS {
                let p = pal(a, s);
                assert_eq!((p.at[0], p.at[3]), (0.0, 1.0));
                assert!(p.at.windows(2).all(|w| w[0] < w[1]));
            }
        }
        assert!(palette(Archetype::None, Season::Summer).is_none());
    }

    #[test]
    fn winter_bare_deciduous_evergreen_conifers() {
        for a in [BroadleafWide, BroadleafNarrow, Birch, Willow, Shrub] {
            assert_eq!(pal(a, Season::Winter).density, 0.0);
        }
        for a in [ConiferConical, Pine] {
            assert_eq!(pal(a, Season::Winter).density, 1.0);
        }
        assert_eq!(pal(Hedge, Season::Winter).density, 0.5);
    }

    #[test]
    fn autumn_warmer_than_summer() {
        let warm = |p: &LeafPalette| {
            (1..=9)
                .map(|i| {
                    let c = leaf_colour(p, i as f32 / 10.0);
                    c[0] - c[1]
                })
                .sum::<f32>()
                / 9.0
        };
        for a in [BroadleafWide, BroadleafNarrow, Birch] {
            assert!(warm(&pal(a, Season::Autumn)) > warm(&pal(a, Season::Summer)) + 0.03);
        }
    }

    #[test]
    fn recolour_keeps_wood_and_detail() {
        let r = leaf_ref(BroadleafWide).unwrap();
        let t = lin([200, 150, 40]);
        assert_eq!(recolour([10, 20, 30], r, t, 0), [10, 20, 30]);
        let dark = recolour(scale(r, 0.6), r, t, 200);
        let light = recolour(scale(r, 1.2), r, t, 200);
        assert!(lum(lin(dark)) < lum(lin(light)));
        let same = recolour(r, r, lin(r), 200);
        assert!((0..3).all(|i| (same[i] as i32 - r[i] as i32).abs() <= 2));
    }

    #[test]
    fn underside_lighter_and_less_saturated() {
        let p = pal(BroadleafWide, Season::Summer);
        let top = leaf_colour(&p, 0.5);
        let u = underside(top, &p);
        let sat = |c: [f32; 3]| {
            let (hi, lo) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
            (hi - lo) / hi
        };
        assert!(lum(u) > lum(top));
        assert!(sat(u) < sat(top));
    }

    #[test]
    fn key_in_range() {
        for i in 0..=10 {
            for j in 0..=10 {
                for k in 0..=10 {
                    let key = gradient_key(i as f32 / 10.0, j as f32 / 10.0, k as f32 / 10.0);
                    assert!((0.0..=1.0).contains(&key));
                }
            }
        }
    }
}
