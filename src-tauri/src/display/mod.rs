mod winapi;

pub use winapi::{
    active_device_names, current_mode, enumerate_displays, enumerate_modes, switch_mode,
    switch_mode_persistent, verify_persistent_mode,
};
