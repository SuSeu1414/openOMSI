//! Developer tool: renders procedural vegetation textures to a PNG sheet.
//!
//! `tex_preview bark <out.png> [--size 512]`
//! `tex_preview leaves <out.png> [--size 1024] [--arch <Name>]`

use glam::{Vec2, Vec3};
use omsi_vegetation::classify::Archetype;
use omsi_vegetation::generator::leaves::HULL;
use omsi_vegetation::tex::bark::{bark, BarkTextures};
use omsi_vegetation::tex::leaves::{atlas_uv, leaves, LeafTextures, TILES};
use std::time::Instant;

const C: usize = 256;
const GAP: usize = 4;
const COLS: usize = 7;
const ROWS: [Archetype; 8] = [
    Archetype::BroadleafWide,
    Archetype::BroadleafNarrow,
    Archetype::ConiferConical,
    Archetype::Pine,
    Archetype::Willow,
    Archetype::Birch,
    Archetype::Shrub,
    Archetype::Hedge,
];

type Cell = Vec<[u8; 3]>;

fn usage(msg: &str) -> ! {
    eprintln!("error: {msg}");
    eprintln!("usage: tex_preview bark <out.png> [--size 512]");
    eprintln!("       tex_preview leaves <out.png> [--size 1024] [--arch <Name>]");
    std::process::exit(2);
}

fn srgb(l: f32) -> u8 {
    (l.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8
}

/// Box-filters a map (colour per texel from `f`, wrapping) down to C x C, repeated `reps`
/// times in each direction; plain nearest when the map is C wide and `reps` is 1.
fn down(size: usize, reps: usize, f: impl Fn(usize, usize) -> [f32; 3]) -> Cell {
    let k = (size * reps / C).max(1);
    let mut out = Vec::with_capacity(C * C);
    for y in 0..C {
        for x in 0..C {
            let mut s = [0.0; 3];
            for (dx, dy) in (0..k).flat_map(|dy| (0..k).map(move |dx| (dx, dy))) {
                let c = f((x * k + dx) % size, (y * k + dy) % size);
                (0..3).for_each(|i| s[i] += c[i]);
            }
            out.push(s.map(|v| (v / (k * k) as f32).round().clamp(0.0, 255.0) as u8));
        }
    }
    out
}

fn px(m: &[u8], size: usize, x: usize, y: usize) -> &[u8] {
    &m[(y * size + x) * 4..(y * size + x) * 4 + 4]
}

fn cells(t: &BarkTextures) -> Vec<Cell> {
    let size = t.size as usize;
    let (ah, nra) = (&t.albedo_height, &t.normal_rough_ao);
    let f = |b: u8| b as f32;
    let alb = |x, y| {
        let p = px(ah, size, x, y);
        [f(p[0]), f(p[1]), f(p[2])]
    };
    let grey = |v: f32| [v, v, v];
    // Tangent-space normal of a texel, z rebuilt.
    let nrm = |x, y| {
        let p = px(nra, size, x, y);
        let (nx, ny) = (p[3] as f32 / 127.5 - 1.0, p[1] as f32 / 127.5 - 1.0);
        Vec3::new(nx, ny, (1.0 - nx * nx - ny * ny).max(0.0).sqrt())
    };
    let l = Vec3::new(-0.8, 0.45, 0.4).normalize();
    let mut cyl = vec![[128u8; 3]; C * C];
    for j in 0..C {
        for i in 0..C {
            let sx = (i as f32 + 0.5) / C as f32 * 2.0 - 1.0;
            if sx.abs() >= 1.0 {
                continue;
            }
            let th = sx.asin();
            let (n0, t0, b0) = (
                Vec3::new(th.sin(), 0.0, th.cos()),
                Vec3::new(th.cos(), 0.0, -th.sin()),
                Vec3::new(0.0, -1.0, 0.0),
            );
            let u = (th / std::f32::consts::TAU + 0.5) * 2.0;
            let v = j as f32 / C as f32 * 2.0;
            let tx = ((u.fract() * size as f32) as usize).min(size - 1);
            let ty = ((v.fract() * size as f32) as usize).min(size - 1);
            let tn = nrm(tx, ty);
            let n = (t0 * tn.x + b0 * tn.y + n0 * tn.z).normalize();
            let ao = px(nra, size, tx, ty)[2] as f32 / 255.0;
            let a = alb(tx, ty).map(|c| (c / 255.0).powf(2.2));
            let k = 0.18 * ao + 1.1 * n.dot(l).max(0.0) * ao.sqrt();
            cyl[j * C + i] = a.map(|c| srgb(c * k));
        }
    }
    vec![
        down(size, 1, alb),
        down(size, 1, |x, y| grey(f(px(ah, size, x, y)[3]))),
        down(size, 1, |x, y| {
            (nrm(x, y) * 0.5 + 0.5).to_array().map(|v| v * 255.0)
        }),
        down(size, 1, |x, y| grey(f(px(nra, size, x, y)[0]))),
        down(size, 1, |x, y| grey(f(px(nra, size, x, y)[2]))),
        down(size, 2, alb),
        cyl,
    ]
}

fn run_bark(args: &[String]) {
    let Some(out) = args.first() else {
        usage("missing <out.png>");
    };
    let mut size = 512u32;
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--size" => {
                size = it
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| usage("bad --size"));
            }
            _ => usage(&format!("unknown option: {a}")),
        }
    }
    if size < 32 || !size.is_power_of_two() {
        usage("--size must be a power of two >= 32");
    }
    let (w, h) = (COLS * (C + GAP) + GAP, ROWS.len() * (C + GAP) + GAP);
    let mut img = vec![128u8; w * h * 3];
    for (r, a) in ROWS.iter().enumerate() {
        let t0 = Instant::now();
        let t = bark(*a, size).expect("archetype has bark");
        println!("row {r}: {a:?} {} ms", t0.elapsed().as_millis());
        for (c, cell) in cells(&t).iter().enumerate() {
            let (ox, oy) = (GAP + c * (C + GAP), GAP + r * (C + GAP));
            for (i, p) in cell.iter().enumerate() {
                let o = ((oy + i / C) * w + ox + i % C) * 3;
                img[o..o + 3].copy_from_slice(p);
            }
        }
    }
    image::save_buffer(out, &img, w as u32, h as u32, image::ColorType::Rgb8)
        .unwrap_or_else(|e| usage(&format!("cannot write {out}: {e}")));
}

const LC: usize = 384;

/// Box-filters an atlas (colour per texel from `f`) down to LC x LC.
fn down_atlas(size: usize, f: impl Fn(usize, usize) -> [f32; 3]) -> Cell {
    let r = size as f32 / LC as f32;
    let span = |i: usize| {
        let a = (i as f32 * r) as usize;
        (a, (((i + 1) as f32 * r) as usize).max(a + 1).min(size))
    };
    let mut out = Vec::with_capacity(LC * LC);
    for y in 0..LC {
        for x in 0..LC {
            let ((x0, x1), (y0, y1)) = (span(x), span(y));
            let mut s = [0.0; 3];
            for (sx, sy) in (y0..y1).flat_map(|sy| (x0..x1).map(move |sx| (sx, sy))) {
                let c = f(sx, sy);
                (0..3).for_each(|i| s[i] += c[i]);
            }
            let n = ((x1 - x0) * (y1 - y0)) as f32;
            out.push(s.map(|v| (v / n).round().clamp(0.0, 255.0) as u8));
        }
    }
    out
}

/// Point-in-convex-polygon on the (counter-clockwise) card hull.
fn in_hull(p: Vec2) -> bool {
    (0..HULL.len()).all(|i| {
        let (a, b) = (HULL[i], HULL[(i + 1) % HULL.len()]);
        (b - a).perp_dot(p - a) >= 0.0
    })
}

/// Share of each tile's hull area (texel centres) with alpha > 128.
fn fills(t: &LeafTextures) -> [f32; 4] {
    let size = t.size as usize;
    let tile = size / TILES as usize;
    let k = 1.0 - 2.0 / 32.0;
    let (mut hull, mut cov) = ([0u32; 4], [0u32; 4]);
    for y in 0..size {
        for x in 0..size {
            let (tx, ty) = (x / tile, y / tile);
            let (lx, ly) = (
                ((x % tile) as f32 + 0.5) / tile as f32,
                ((y % tile) as f32 + 0.5) / tile as f32,
            );
            let c = Vec2::new((lx - 1.0 / 64.0) / k, 1.0 - (ly - 1.0 / 64.0) / k);
            if in_hull(c) {
                let i = ty * TILES as usize + tx;
                hull[i] += 1;
                cov[i] += (t.albedo_alpha[(y * size + x) * 4 + 3] > 128) as u32;
            }
        }
    }
    [0, 1, 2, 3].map(|i| cov[i] as f32 / hull[i].max(1) as f32)
}

fn leaf_cells(t: &LeafTextures) -> Vec<Cell> {
    let size = t.size as usize;
    let (aa, nrt) = (&t.albedo_alpha, &t.normal_rough_trans);
    let f = |b: u8| b as f32;
    let grey = |v: f32| [v, v, v];
    let sky = [170.0, 200.0, 235.0];
    let checker = |x: usize, y: usize| {
        let v = if ((x * LC / size) / 16 + (y * LC / size) / 16).is_multiple_of(2) {
            96.0
        } else {
            140.0
        };
        [v, v, v]
    };
    let blend =
        |fg: [f32; 3], bg: [f32; 3], a: f32| [0, 1, 2].map(|i| fg[i] * a + bg[i] * (1.0 - a));
    let mut alb = down_atlas(size, |x, y| {
        let p = px(aa, size, x, y);
        blend([f(p[0]), f(p[1]), f(p[2])], checker(x, y), f(p[3]) / 255.0)
    });
    // Hull outline of every tile in red.
    for tile in 0..TILES * TILES {
        for i in 0..HULL.len() {
            let (a, b) = (HULL[i], HULL[(i + 1) % HULL.len()]);
            for s in 0..=400 {
                let p = atlas_uv(a.lerp(b, s as f32 / 400.0), tile) * LC as f32;
                let (x, y) = (p.x as usize, p.y as usize);
                if x < LC && y < LC {
                    alb[y * LC + x] = [255, 0, 0];
                }
            }
        }
    }
    let nrm = |x, y| {
        let p = px(nrt, size, x, y);
        let (nx, ny) = (p[3] as f32 / 127.5 - 1.0, p[1] as f32 / 127.5 - 1.0);
        Vec3::new(nx, ny, (1.0 - nx * nx - ny * ny).max(0.0).sqrt())
    };
    let warm = [1.1, 1.05, 0.6];
    vec![
        alb,
        down_atlas(size, |x, y| grey(f(px(aa, size, x, y)[3]))),
        down_atlas(size, |x, y| {
            (nrm(x, y) * 0.5 + 0.5).to_array().map(|v| v * 255.0)
        }),
        down_atlas(size, |x, y| grey(f(px(nrt, size, x, y)[0]))),
        down_atlas(size, |x, y| grey(f(px(nrt, size, x, y)[2]))),
        down_atlas(size, |x, y| {
            let (p, q) = (px(aa, size, x, y), px(nrt, size, x, y));
            let tr = f(q[2]) / 255.0;
            let lit = [0, 1, 2].map(|i| {
                let a = (f(p[i]) / 255.0).powf(2.2) * (0.25 + 1.6 * tr * warm[i]);
                a.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0
            });
            blend(lit, sky, f(p[3]) / 255.0)
        }),
    ]
}

fn run_leaves(args: &[String]) {
    let Some(out) = args.first() else {
        usage("missing <out.png>");
    };
    let mut size = 1024u32;
    let mut only: Option<String> = None;
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--size" => {
                size = it
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_else(|| usage("bad --size"));
            }
            "--arch" => only = Some(it.next().cloned().unwrap_or_else(|| usage("bad --arch"))),
            _ => usage(&format!("unknown option: {a}")),
        }
    }
    if size < 128 || !size.is_power_of_two() {
        usage("--size must be a power of two >= 128");
    }
    let rows: Vec<Archetype> = ROWS
        .iter()
        .copied()
        .filter(|a| leaves(*a, 128).is_some())
        .filter(|a| only.as_ref().is_none_or(|n| *n == format!("{a:?}")))
        .collect();
    if rows.is_empty() {
        usage("no broadleaf archetype matches --arch");
    }
    let (w, h) = (6 * (LC + GAP) + GAP, rows.len() * (LC + GAP) + GAP);
    let mut img = vec![128u8; w * h * 3];
    for (r, a) in rows.iter().enumerate() {
        let t0 = Instant::now();
        let t = leaves(*a, size).expect("broadleaf archetype");
        let ms = t0.elapsed().as_millis();
        let fl = fills(&t).map(|v| format!("{v:.2}"));
        println!("row {r}: {a:?} {ms} ms fill [{}]", fl.join(" "));
        for (c, cell) in leaf_cells(&t).iter().enumerate() {
            let (ox, oy) = (GAP + c * (LC + GAP), GAP + r * (LC + GAP));
            for (i, p) in cell.iter().enumerate() {
                let o = ((oy + i / LC) * w + ox + i % LC) * 3;
                img[o..o + 3].copy_from_slice(p);
            }
        }
    }
    image::save_buffer(out, &img, w as u32, h as u32, image::ColorType::Rgb8)
        .unwrap_or_else(|e| usage(&format!("cannot write {out}: {e}")));
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("bark") => run_bark(&args[1..]),
        Some("leaves") => run_leaves(&args[1..]),
        Some(other) => usage(&format!("unknown subcommand: {other}")),
        None => usage("missing subcommand"),
    }
}
