//! The leaf alpha must survive the coverage-keeping mip chain and BC3 on the real atlases.

use omsi_texture::bc::{self, Bc};
use omsi_texture::gpu::{prepare_vegetation_with, GpuOptions};
use omsi_texture::Image;
use omsi_vegetation::classify::Archetype;
use omsi_vegetation::tex::leaves::{leaf_ref, leaves};

const ALL: [Archetype; 8] = [
    Archetype::BroadleafWide,
    Archetype::BroadleafNarrow,
    Archetype::ConiferConical,
    Archetype::Pine,
    Archetype::Willow,
    Archetype::Birch,
    Archetype::Shrub,
    Archetype::Hedge,
];

#[test]
fn atlas_coverage_survives_mips_and_bc3() {
    let mut failures = Vec::new();
    for a in ALL.into_iter().filter(|a| leaf_ref(*a).is_some()) {
        let t = leaves(a, 256).expect("leaf archetype");
        let img = Image {
            rgba: t.albedo_alpha.clone(),
            width: 256,
            height: 256,
            has_alpha: true,
        };
        let c0 = bc::alpha_coverage(&img.rgba);
        let o = GpuOptions {
            bc: true,
            compress: true,
        };
        let kept = prepare_vegetation_with(img.clone(), o);
        let (mut plain, mut side) = (img.rgba, 256u32);
        let (mut kl, mut pl) = (String::new(), String::new());
        for (i, blocks) in kept.levels.iter().enumerate() {
            if i > 0 {
                plain = bc::downsample(&plain, side, side).0;
                side /= 2;
            }
            if side < 16 {
                break;
            }
            let c = bc::alpha_coverage(&bc::decode(blocks, side, side, Bc::Bc3));
            let pc = bc::alpha_coverage(&bc::decode(
                &bc::encode(&plain, side, side, Bc::Bc3).0,
                side,
                side,
                Bc::Bc3,
            ));
            kl += &format!(" {side}:{c:.3}");
            pl += &format!(" {side}:{pc:.3}");
            if c < 0.85 * c0 || c > 1.15 * c0 {
                failures.push(format!("{a:?} side {side}: {c:.3} vs c0 {c0:.3}"));
            }
        }
        eprintln!(
            "{a:?} c0 {c0:.3} kept [{}] plain [{}]",
            kl.trim(),
            pl.trim()
        );
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
