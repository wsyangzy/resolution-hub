mod winapi;

pub use winapi::{
    current_mode, enumerate_displays, enumerate_modes, switch_mode, switch_mode_persistent,
};
