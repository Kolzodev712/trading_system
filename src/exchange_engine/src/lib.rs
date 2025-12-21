// Ensure standard library core is accessible for macro expansion
extern crate core as std_core;

pub mod config;
pub mod connection_manager;
pub mod exchanges;
pub mod run;
pub mod state;
