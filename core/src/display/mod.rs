mod winapi;

pub use winapi::{
    current_mode, enumerate_displays, enumerate_modes, fit_windows_to_desktop, registry_mode,
    switch_mode, switch_mode_persistent, verify_persistent_mode,
};
