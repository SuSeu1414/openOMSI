//! Plain `f32` triangle mesh produced by the tree generator.

use glam::{Vec2, Vec3, Vec4};

use crate::seed::splitmix64;

/// Per-vertex data for the wind and leaf shaders; plain `f32`, the renderer packs it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VertexAttrs {
    /// Wind pivot of the level-1 ancestor-or-self stem: xyz = its points[0], w = its length.
    /// `Vec4::ZERO` for vertices of level-0 stems (trunk and fork leaders).
    pub branch: Vec4,
    /// Wind pivot of the level-2 ancestor-or-self stem, same layout; equals `branch` when the
    /// stem's level is <= 1. Leaf cards: the stem the card sits on, with the same rule.
    pub twig: Vec4,
    /// Unit normal of the crown ellipsoid at the vertex position.
    pub crown_normal: Vec3,
    /// Leaf flutter weight: the hull v of the vertex (0 at the card base); 0 for bark.
    pub flutter: f32,
    /// Crown-depth occlusion in [0, 1], 1 = open.
    pub ao: f32,
    /// [0, 1), shared by all vertices on one level-1 branch and its descendants; 0 on level-0 stems.
    pub phase: f32,
    /// [0, 1]: sqrt(stem.radii[0] / trunk radii[0]) clamped; leaf vertices 0.
    pub stiffness: f32,
    /// [0, 1): per stem for bark, per card for leaves.
    pub rand: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<Vec3>,
    /// Unit length.
    pub normals: Vec<Vec3>,
    /// Bark: u around, v along; both in texture repeats.
    pub uvs: Vec<Vec2>,
    /// Triangle list, counter-clockwise seen from outside.
    pub indices: Vec<u32>,
    /// One per position (may be empty only for meshes built by hand).
    pub attrs: Vec<VertexAttrs>,
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
        for a in &self.attrs {
            let floats = [a.branch.to_array().as_slice(), &a.twig.to_array()]
                .concat()
                .into_iter()
                .chain(a.crown_normal.to_array())
                .chain([a.flutter, a.ao, a.phase, a.stiffness, a.rand]);
            floats.for_each(|x| f(x.to_bits() as u64));
        }
        self.indices.iter().for_each(|&i| f(i as u64));
        d
    }
}
