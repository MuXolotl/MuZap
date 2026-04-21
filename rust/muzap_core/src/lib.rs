pub mod app;
pub mod config_files;
pub mod ini;
pub mod params;
pub mod paths;
pub mod print;
pub mod process;

pub mod error;

#[cfg(windows)]
pub mod win;

pub use error::{CoreError, CoreResult};

/// Заготовки под будущий launcher — отключено по умолчанию.
#[cfg(feature = "launcher")]
pub mod launcher_prelude;
