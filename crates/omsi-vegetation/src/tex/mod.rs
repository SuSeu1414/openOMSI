//! Building blocks of the procedural vegetation textures: periodic noise (`noise`) and field
//! operations (`ops`). Pure data, deterministic from a seed; every noise is periodic so a
//! texture built from it tiles without a seam. `bark` builds the per-archetype bark maps.

pub mod bark;
pub mod noise;
pub mod ops;

/// A square scalar field, row-major, top-left origin (row = +y = texture v going down).
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub size: u32,
    pub v: Vec<f32>,
}

impl Field {
    /// A field of zeros.
    pub fn new(size: u32) -> Field {
        Field {
            size,
            v: vec![0.0; (size * size) as usize],
        }
    }

    /// A field with `f(x, y)` in every texel.
    pub fn from_fn(size: u32, mut f: impl FnMut(u32, u32) -> f32) -> Field {
        let mut v = Vec::with_capacity((size * size) as usize);
        for y in 0..size {
            for x in 0..size {
                v.push(f(x, y));
            }
        }
        Field { size, v }
    }

    /// Texel with wrap-around (`wrap`) or clamp-to-edge.
    pub fn at(&self, x: i32, y: i32, wrap: bool) -> f32 {
        let n = self.size as i32;
        let (x, y) = if wrap {
            (x.rem_euclid(n), y.rem_euclid(n))
        } else {
            (x.clamp(0, n - 1), y.clamp(0, n - 1))
        };
        self.v[(y * n + x) as usize]
    }
}
