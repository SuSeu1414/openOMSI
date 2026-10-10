//! Field operations: normals from height, cavity occlusion, colour dilation, 8-bit encoding.

use super::Field;
use glam::Vec3;

/// Tangent-space normals from a height field: +x = increasing column, +y = increasing row,
/// +z out of the surface. `strength` = height units per texel step.
pub fn normal_from_height(h: &Field, strength: f32, wrap: bool) -> Vec<Vec3> {
    let n = h.size as i32;
    let mut out = Vec::with_capacity(h.v.len());
    for y in 0..n {
        for x in 0..n {
            let dx = (h.at(x + 1, y, wrap) - h.at(x - 1, y, wrap)) * 0.5;
            let dy = (h.at(x, y + 1, wrap) - h.at(x, y - 1, wrap)) * 0.5;
            out.push(Vec3::new(-strength * dx, -strength * dy, 1.0).normalize());
        }
    }
    out
}

/// Cavity occlusion: 1 on ridges/flats, darker where the texel lies below its blurred
/// neighbourhood. Result in [0, 1].
pub fn cavity_ao(h: &Field, radius: u32, strength: f32, wrap: bool) -> Field {
    let r = radius as i32;
    let inv = 1.0 / (2 * r + 1) as f32;
    let blur = |src: &Field, horizontal: bool| {
        Field::from_fn(h.size, |x, y| {
            let (x, y) = (x as i32, y as i32);
            let mut s = 0.0;
            for k in -r..=r {
                s += if horizontal {
                    src.at(x + k, y, wrap)
                } else {
                    src.at(x, y + k, wrap)
                };
            }
            s * inv
        })
    };
    let b = blur(&blur(h, true), false);
    Field::from_fn(h.size, |x, y| {
        let i = (y * h.size + x) as usize;
        (1.0 - strength * (b.v[i] - h.v[i]).max(0.0)).clamp(0.0, 1.0)
    })
}

/// Fills the channels of texels with `mask[i] == 0` from their filled neighbours so mips and
/// BC blocks never pull in black. `channels` = how many leading channels of each RGBA texel
/// are filled (3 for albedo+coverage alpha, 4 for data maps); the rest stay untouched.
pub fn dilate(rgba: &mut [u8], mask: &[u8], size: u32, channels: usize, passes: u32) {
    assert_eq!(mask.len(), (size * size) as usize, "dilate: mask length");
    assert_eq!(rgba.len(), 4 * mask.len(), "dilate: rgba length");
    let n = size as i32;
    let channels = channels.min(4);
    let mut filled: Vec<bool> = mask.iter().map(|&m| m > 0).collect();
    let mut total = [0u64; 4];
    let mut count = 0u64;
    for i in (0..filled.len()).filter(|&i| filled[i]) {
        count += 1;
        for (c, t) in total.iter_mut().enumerate().take(channels) {
            *t += rgba[i * 4 + c] as u64;
        }
    }
    for _ in 0..passes {
        let prev = filled.clone();
        for y in 0..n {
            for x in 0..n {
                let i = (y * n + x) as usize;
                if prev[i] {
                    continue;
                }
                let mut sum = [0u32; 4];
                let mut cnt = 0u32;
                for (dx, dy) in (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (dx, dy))) {
                    let (nx, ny) = (x + dx, y + dy);
                    if (dx, dy) == (0, 0) || nx < 0 || ny < 0 || nx >= n || ny >= n {
                        continue;
                    }
                    let j = (ny * n + nx) as usize;
                    if prev[j] {
                        cnt += 1;
                        for (c, s) in sum.iter_mut().enumerate().take(channels) {
                            *s += rgba[j * 4 + c] as u32;
                        }
                    }
                }
                if cnt > 0 {
                    for (c, s) in sum.iter().enumerate().take(channels) {
                        rgba[i * 4 + c] = (*s as f32 / cnt as f32).round() as u8;
                    }
                    filled[i] = true;
                }
            }
        }
    }
    for i in (0..filled.len()).filter(|&i| !filled[i]) {
        for (c, t) in total.iter().enumerate().take(channels) {
            rgba[i * 4 + c] = if count > 0 {
                (*t as f32 / count as f32).round() as u8
            } else {
                0
            };
        }
    }
}

/// `clamp(v, 0, 1) * 255`, rounded.
pub fn unorm8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// `n * 0.5 + 0.5` per component, as unorm8.
pub fn encode_normal(n: Vec3) -> [u8; 3] {
    let e = n * 0.5 + 0.5;
    [unorm8(e.x), unorm8(e.y), unorm8(e.z)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_height_gives_up_normal() {
        let h = Field::new(8);
        for n in normal_from_height(&h, 2.0, false) {
            assert_eq!(n, Vec3::Z);
            let e = encode_normal(n);
            assert!(e[0].abs_diff(128) <= 1 && e[1].abs_diff(128) <= 1 && e[2] == 255);
        }
    }

    #[test]
    fn ramp_normal_leans_back() {
        let h = Field::from_fn(8, |x, _| x as f32 / 8.0);
        let ns = normal_from_height(&h, 1.0, false);
        for y in 0..8 {
            for x in 1..7 {
                let n = ns[y * 8 + x];
                assert!(n.x < 0.0 && n.y == 0.0);
            }
        }
    }

    #[test]
    fn wrapped_normals_seamless() {
        let n = 16u32;
        let w = std::f32::consts::TAU / n as f32;
        let h = Field::from_fn(n, |x, _| (x as f32 * w).sin());
        let ns = normal_from_height(&h, 1.0, true);
        // At x = 0 the wrapped neighbour is column N-1, i.e. sin(-w).
        let dx = (w.sin() - (-w).sin()) * 0.5;
        let expected = Vec3::new(-dx, 0.0, 1.0).normalize();
        assert!((ns[0] - expected).length() < 1e-4);
    }

    #[test]
    fn cavity_flat_is_open_pit_is_dark() {
        let flat = cavity_ao(&Field::new(16), 2, 4.0, true);
        assert!(flat.v.iter().all(|&v| v == 1.0));
        let mut h = Field::new(16);
        h.v[8 * 16 + 8] = -1.0;
        let ao = cavity_ao(&h, 2, 4.0, true);
        assert!(ao.v[8 * 16 + 8] < 1.0);
        assert_eq!(ao.v[0], 1.0);
    }

    #[test]
    fn dilate_one_pass_grows_one_texel() {
        let mut img = vec![0u8; 8 * 8 * 4];
        let mut mask = vec![0u8; 64];
        for (x, y, c) in [(1, 1, [255, 0, 0]), (6, 6, [0, 0, 255])] {
            let i = y * 8 + x;
            img[i * 4..i * 4 + 3].copy_from_slice(&c);
            mask[i] = 255;
        }
        dilate(&mut img, &mask, 8, 3, 1);
        let px = |x: usize, y: usize| &img[(y * 8 + x) * 4..(y * 8 + x) * 4 + 3];
        assert_eq!(px(2, 2), [255, 0, 0]);
        assert_eq!(px(5, 5), [0, 0, 255]);
        assert_eq!(px(4, 3), [128, 0, 128]);
    }

    #[test]
    fn dilate_spreads_colour_keeps_alpha() {
        let mut img = vec![0u8; 16 * 16 * 4];
        let src = 5 * 16 + 9;
        img[src * 4..src * 4 + 4].copy_from_slice(&[200, 30, 10, 255]);
        let mask: Vec<u8> = img.chunks(4).map(|t| t[3]).collect();
        dilate(&mut img, &mask, 16, 3, 16);
        for (i, t) in img.chunks(4).enumerate() {
            assert_eq!(&t[..3], &[200, 30, 10]);
            assert_eq!(t[3], if i == src { 255 } else { 0 });
        }
        let mut img = vec![0u8; 16 * 16 * 4];
        img[src * 4..src * 4 + 4].copy_from_slice(&[1, 2, 3, 4]);
        let mut mask = vec![0u8; 256];
        mask[src] = 1;
        dilate(&mut img, &mask, 16, 4, 16);
        assert!(img.chunks(4).all(|t| t == [1, 2, 3, 4]));
    }
}
