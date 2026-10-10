//! Developer tool: renders procedural vegetation textures to a PNG sheet.
//!
//! `tex_preview bark <out.png> [--size 512]`

use glam::Vec3;
use omsi_vegetation::classify::Archetype;
use omsi_vegetation::tex::bark::{bark, BarkTextures};
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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("bark") => run_bark(&args[1..]),
        Some(other) => usage(&format!("unknown subcommand: {other}")),
        None => usage("missing subcommand"),
    }
}
