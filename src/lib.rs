#![deny(clippy::all)]

mod backends;
mod db;
mod error;
mod settings;
mod wrapper;

pub use db::Database;
pub use settings::{Settings, WrapperSettings};
