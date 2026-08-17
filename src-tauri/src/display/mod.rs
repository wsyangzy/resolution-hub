mod winapi;

pub use winapi::{
    active_device_names, current_mode, enumerate_displays, enumerate_modes, fit_windows_to_desktop,
    switch_mode, switch_mode_persistent, verify_persistent_mode,
};
