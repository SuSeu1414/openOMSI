//! Vegetation: what a placed tree or plant is (`classify`) and the per-instance seed and
//! variation derived from it (`seed`), plus the procedural Weber-Penn tree skeleton (`generator`,
//! `presets`), the on-disk cache of generated meshes (`cache`) and the building blocks of the
//! procedural textures (`tex`). Pure data: no GPU, no clock.

pub mod cache;
pub mod classify;
pub mod generator;
pub mod overrides;
pub mod presets;
pub mod seed;
pub mod tex;
