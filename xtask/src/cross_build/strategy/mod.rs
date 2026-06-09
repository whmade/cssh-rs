//! Per-target build orchestration.
//!
//! One submodule per supported target. Each exposes a single
//! `build(system, release)` entry point that the top-level
//! [`super::run_cross_build`] dispatches to.

pub mod windows_msvc;
