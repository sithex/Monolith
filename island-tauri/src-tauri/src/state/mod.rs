//! Phase 3 state authority: the pure reducer (`machine`), the single-owner actor (`actor`) and
//! its host-side effect sink + managed handle (`host`). SPEC §4.1 / §8 Phase 3 (revised after
//! the Phase 3 review): the actor is the only `IslandState` authority; the renderer reads it.
pub mod actor;
pub mod host;
pub mod machine;
