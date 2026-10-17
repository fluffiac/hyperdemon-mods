//! Minimal mod: loads, logs one line, and changes nothing in the game.
//! Copy this folder to start a new mod. Every other `Mod` callback
//! (`tick`, `ui`, `frame`, `world_tick`, ...) has an empty default.

use hd_sdk::prelude::*;

struct Example;

impl Mod for Example {
    const NAME: &'static str = "example";
    const VERSION: &'static str = "0.1.0";

    fn init() -> Self {
        log_info!("loaded");
        Example
    }
}

hd_sdk::hd_mod!(Example);
