//! Procedural tree generation (Weber-Penn): parameters, the stem skeleton and its tube mesh.

pub mod mesh;
pub mod params;
mod rings;
pub mod skeleton;
pub mod tube;

pub use mesh::*;
pub use params::*;
pub use skeleton::*;
pub use tube::*;
