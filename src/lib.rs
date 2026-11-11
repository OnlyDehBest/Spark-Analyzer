#![allow(
    clippy::needless_range_loop,
    clippy::unnecessary_sort_by,
    clippy::redundant_guards
)]

#[macro_use]
pub mod i18n;
pub mod alloc;
pub mod analysis;
#[cfg(feature = "cli")]
pub mod archive;
pub mod config;
pub mod diff;
pub mod flame;
pub mod heap;
pub mod heapdiff;
pub mod hprof;
pub mod loader;
pub mod model;
pub mod patterns;
pub mod pipeline;
pub mod proto;
pub mod report;
pub mod rules;
#[cfg(feature = "cli")]
pub mod server;
#[cfg(feature = "cli")]
pub mod usage;
#[cfg(feature = "cli")]
pub mod wasm_bundle;
pub mod webui;
