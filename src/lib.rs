#![deny(clippy::all)]

mod backends;
mod error;
mod settings;
mod wrapper;

pub use settings::{Settings, WrapperSettings};

// Legacy modules retained temporarily for reference; will be removed end of Phase 2.
mod couch;
mod dirty;
mod general;
mod memory;
mod postgres;
mod sqlite;
mod utils;

#[macro_use]
extern crate napi_derive;
