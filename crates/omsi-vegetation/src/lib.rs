//! Vegetation: what a placed tree or plant is (`classify`) and the per-instance seed and
//! variation derived from it (`seed`), plus the procedural Weber-Penn tree skeleton (`gen`,
//! `presets`). Pure data: no GPU, no clock.

pub mod classify;
pub mod generator;
pub mod overrides;
pub mod presets;
pub mod seed;
