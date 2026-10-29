//! ISEP Moodle Assistant — núcleo e API local.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod ai;
pub mod config;
pub mod history;
pub mod http;
pub mod model;
pub mod paths;
pub mod util;
pub mod moodle_report;
pub mod summarizer;
pub mod dpapi;
pub mod mail;
pub mod graph;
pub mod session;
pub mod system;
pub mod moodle_sync;
pub mod progress;
pub mod digest;
pub mod api;
pub mod service;
pub mod views;
