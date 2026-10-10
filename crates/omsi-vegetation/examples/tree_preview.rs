//! Developer tool: renders generated trees (front, side, top) to a PNG with a CPU rasteriser.
//!
//! `tree_preview <archetype> <height> <width> <out.png> [--seeds N] [--seed0 S]
//!  [--min-radius R] [--size PX] [--no-leaves] [--ao]`

use glam::{Vec2, Vec3};
use omsi_vegetation::classify::Archetype;
use omsi_vegetation::generator::{leaves, skeleton, tubes, Target, TubeOptions};
use omsi_vegetation::presets::preset;

const BG: [u8; 3] = [205, 214, 224];
const BARK: [f32; 3] = [110.0, 80.0, 55.0];
const LEAF: [f32; 3] = [70.0, 120.0, 50.0];
const RED: [u8; 3] = [220, 40, 40];

struct Canvas {
    w: usize,
    img: Vec<[u8; 3]>,
}

impl Canvas {
    fn put(&mut self, x: i32, y: i32, c: [u8; 3]) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.img.len() / self.w {
            self.img[y as usize * self.w + x as usize] = c;
        }
    }

    fn line(&mut self, a: Vec2, b: Vec2, c: [u8; 3]) {
        let n = (b - a).abs().max_element().ceil().max(1.0) as i32;
        for i in 0..=n {
            let p = a.lerp(b, i as f32 / n as f32);
            self.put(p.x.floor() as i32, p.y.floor() as i32, c);
        }
    }
}

fn usage(msg: &str) -> ! {
    eprintln!("error: {msg}");
    eprintln!(
        "usage: tree_preview <archetype> <height> <width> <out.png> [--seeds N] [--seed0 S] \
         [--min-radius R] [--size PX] [--no-leaves] [--ao]"
    );
    std::process::exit(2);
}

fn num<T: std::str::FromStr>(s: &str, what: &str) -> T {
    s.parse()
        .unwrap_or_else(|_| usage(&format!("bad {what}: {s}")))
}

/// Twice the signed area of triangle (a, b, c) in screen xy.
fn edge(a: Vec3, b: Vec3, c: Vec3) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 4 {
        usage("missing arguments");
    }
    let arch = Archetype::from_name(&args[0])
        .unwrap_or_else(|| usage(&format!("unknown archetype: {}", args[0])));
    let p = preset(arch).unwrap_or_else(|| usage(&format!("no preset for {}", args[0])));
    let height: f32 = num(&args[1], "height");
    let width: f32 = num(&args[2], "width");
    let out = &args[3];
    let (mut seeds, mut seed0, mut min_radius, mut size) = (3u32, 1u64, 0.0f32, 512usize);
    let no_leaves = args[4..].iter().any(|a| a == "--no-leaves");
    let ao = args[4..].iter().any(|a| a == "--ao");
    let rest: Vec<&String> = args[4..]
        .iter()
        .filter(|a| *a != "--no-leaves" && *a != "--ao")
        .collect();
    let mut it = rest.into_iter();
    while let Some(k) = it.next() {
        let v = it
            .next()
            .unwrap_or_else(|| usage(&format!("{k} needs a value")));
        match k.as_str() {
            "--seeds" => seeds = num(v, "--seeds"),
            "--seed0" => seed0 = num(v, "--seed0"),
            "--min-radius" => min_radius = num(v, "--min-radius"),
            "--size" => size = num(v, "--size"),
            _ => usage(&format!("unknown option: {k}")),
        }
    }
    if !(height > 0.0 && width > 0.0 && seeds > 0 && size > 0) {
        usage("height, width, --seeds and --size must be positive");
    }

    let pxf = size as f32;
    let scale = 0.9 * pxf / height.max(width * 1.2);
    let mut cv = Canvas {
        w: 3 * size,
        img: vec![BG; 3 * size * seeds as usize * size],
    };
    let opts = TubeOptions {
        min_radius,
        ..Default::default()
    };
    let light = Vec3::new(-0.4, -0.6, 0.7).normalize();

    for j in 0..seeds {
        let seed = seed0 + j as u64;
        let sk = skeleton(&p, Target { height, width }, seed);
        let mesh = tubes(&sk, &opts, seed);
        let leaf = if no_leaves {
            Default::default()
        } else {
            leaves(&sk, &p.leaves, seed)
        };
        let mut cnt = [0usize; 4];
        for s in &sk.stems {
            cnt[(s.level as usize).min(3)] += 1;
        }
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for &v in mesh.positions.iter().chain(&leaf.positions) {
            lo = lo.min(v);
            hi = hi.max(v);
        }
        println!(
            "seed {seed}: stems L0/L1/L2/L3 = {}/{}/{}/{}, triangles {}, cards {}, bbox min ({:.2},{:.2},{:.2}) max ({:.2},{:.2},{:.2})",
            cnt[0], cnt[1], cnt[2], cnt[3], mesh.triangles(), leaf.positions.len() / 8, lo.x, lo.y, lo.z, hi.x, hi.y, hi.z
        );

        let row_y = j as usize * size;
        let py0 = row_y as f32;
        for panel in 0..3 {
            let px0 = (panel * size) as f32;
            let origin = if panel == 2 {
                Vec2::new(px0 + pxf / 2.0, py0 + pxf / 2.0)
            } else {
                Vec2::new(px0 + pxf / 2.0, py0 + pxf - 0.05 * pxf)
            };
            // World to (screen x, screen y, depth).
            let proj = |v: Vec3| -> Vec3 {
                let (h, u, d) = match panel {
                    0 => (v.x, v.z, v.y),
                    1 => (v.y, v.z, -v.x),
                    _ => (v.x, v.y, -v.z),
                };
                Vec3::new(origin.x + h * scale, origin.y - u * scale, d)
            };
            let mut depth = vec![f32::MAX; size * size];
            for (m, base) in [(&mesh, BARK), (&leaf, LEAF)] {
                for t in m.indices.as_chunks::<3>().0 {
                    let w = t.map(|i| m.positions[i as usize]);
                    let n = (w[1] - w[0]).cross(w[2] - w[0]).normalize_or_zero();
                    let mut k = 0.25 + 0.75 * n.dot(light).abs();
                    if ao {
                        k *= t.iter().map(|&i| m.attrs[i as usize].ao).sum::<f32>() / 3.0;
                    }
                    let col = base.map(|c| (c * k).round().clamp(0.0, 255.0) as u8);
                    let s = w.map(proj);
                    let area = edge(s[0], s[1], s[2]);
                    if area.abs() < 1e-9 {
                        continue;
                    }
                    let lo = s[0].min(s[1]).min(s[2]);
                    let hi = s[0].max(s[1]).max(s[2]);
                    let x0 = lo.x.floor().max(px0) as i32;
                    let x1 = hi.x.ceil().min(px0 + pxf - 1.0) as i32;
                    let y0 = lo.y.floor().max(py0) as i32;
                    let y1 = hi.y.ceil().min(py0 + pxf - 1.0) as i32;
                    for y in y0..=y1 {
                        for x in x0..=x1 {
                            let c = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, 0.0);
                            let w0 = edge(s[1], s[2], c) / area;
                            let w1 = edge(s[2], s[0], c) / area;
                            let w2 = edge(s[0], s[1], c) / area;
                            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                                continue;
                            }
                            let z = w0 * s[0].z + w1 * s[1].z + w2 * s[2].z;
                            let di = (y - row_y as i32) as usize * size + (x - px0 as i32) as usize;
                            if z < depth[di] {
                                depth[di] = z;
                                cv.put(x, y, col);
                            }
                        }
                    }
                }
            }
            // Overlay: ground line and target box.
            let pt = |h: f32, u: f32| Vec2::new(origin.x + h * scale, origin.y - u * scale);
            if panel == 2 {
                let r = width / 2.0;
                let at = |i: u32| {
                    let a = i as f32 / 64.0 * std::f32::consts::TAU;
                    pt(r * a.cos(), r * a.sin())
                };
                for i in 0..64 {
                    cv.line(at(i), at(i + 1), RED);
                }
            } else {
                let gy = pt(0.0, 0.0).y;
                cv.line(
                    Vec2::new(px0, gy),
                    Vec2::new(px0 + pxf - 1.0, gy),
                    [90, 110, 70],
                );
                let hw = width / 2.0;
                let c = [pt(-hw, 0.0), pt(hw, 0.0), pt(hw, height), pt(-hw, height)];
                for i in 0..4 {
                    cv.line(c[i], c[(i + 1) % 4], RED);
                }
            }
        }
    }

    let flat: Vec<u8> = cv.img.iter().flatten().copied().collect();
    let h = (cv.img.len() / cv.w) as u32;
    image::save_buffer(out, &flat, cv.w as u32, h, image::ColorType::Rgb8).unwrap_or_else(|e| {
        eprintln!("error: writing {out}: {e}");
        std::process::exit(1);
    });
}
