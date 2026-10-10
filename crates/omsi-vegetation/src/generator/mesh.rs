//! Plain `f32` triangle mesh produced by the tree generator.

use glam::{Vec2, Vec3};

use crate::seed::splitmix64;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<Vec3>,
    /// Unit length.
    pub normals: Vec<Vec3>,
    /// Bark: u around, v along; both in texture repeats.
    pub uvs: Vec<Vec2>,
    /// Triangle list, counter-clockwise seen from outside.
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }

    /// Bit-exact digest: splitmix64 fold over `to_bits()` of every float and every index, in order.
    pub fn digest(&self) -> u64 {
        let mut d = 0u64;
        let mut f = |x: u64| d = splitmix64(d ^ x);
        for v in self.positions.iter().chain(&self.normals) {
            v.to_array().iter().for_each(|x| f(x.to_bits() as u64));
        }
        for v in &self.uvs {
            v.to_array().iter().for_each(|x| f(x.to_bits() as u64));
        }
        self.indices.iter().for_each(|&i| f(i as u64));
        d
    }
}
