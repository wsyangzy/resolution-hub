use std::{cell::Cell, rc::Rc};

use slint::{
    ComponentHandle,
    winit_030::{
        EventResult, WinitWindowAccessor,
        winit::{
            dpi::{LogicalPosition, PhysicalPosition},
            event::WindowEvent,
            platform::windows::WindowAttributesExtWindows,
            raw_window_handle::{HasWindowHandle, RawWindowHandle},
            window::{Theme, Window, WindowButtons},
        },
    },
};
use windows::Win32::{
    Foundation::HWND,
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow},
    UI::WindowsAndMessaging::{
        GW_OWNER, GWLP_HWNDPARENT, GetWindow, SPI_GETCLIENTAREAANIMATION,
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetWindowLongPtrW, SystemParametersInfoW,
    },
};

use crate::{
    AppWindow,
    placement::{Anchor, Bounds, place_panel, place_window_panel},
    state::Panel,
};

pub struct WindowFactory(Rc<Cell<bool>>);

pub enum NativeEvent {
    Deactivated,
    GeometryChanged,
    PanelPainted(PanelFrame),
}

/// 已绘制帧的内容及物理尺寸。进入回调排队期间更换内容或尺寸时，旧帧不能放行。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelFrame {
    kind: i32,
    size: slint::PhysicalSize,
}

impl PanelFrame {
    fn ready(
        kind: i32,
        target: slint::LogicalSize,
        scale: f64,
        rendered: slint::PhysicalSize,
        native: slint::PhysicalSize,
    ) -> Option<Self> {
        let expected = target.to_physical(scale as f32);
        (expected.width > 0 && expected.height > 0 && rendered == expected && native == expected)
            .then_some(Self {
                kind,
                size: expected,
            })
    }
}

pub fn panel_frame(panel: &AppWindow) -> Option<PanelFrame> {
    if !panel.get_panel_geometry_ready() {
        return None;
    }
    panel
        .window()
        .with_winit_window(|native| {
            let native_size = native.inner_size();
            PanelFrame::ready(
                panel.get_panel_kind(),
                slint::LogicalSize::new(panel.get_panel_width(), panel.get_panel_height()),
                native.scale_factor(),
                panel.window().size(),
                slint::PhysicalSize::new(native_size.width, native_size.height),
            )
        })
        .flatten()
}

pub fn select_backend() -> Result<WindowFactory, slint::PlatformError> {
    let creating_panel = Rc::new(Cell::new(false));
    let role = creating_panel.clone();
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_window_attributes_hook(move |attributes| {
            attributes
                .with_theme(Some(Theme::Light))
                .with_undecorated_shadow(true)
                .with_resizable(false)
                .with_skip_taskbar(role.get())
                .with_enabled_buttons(if role.get() {
                    WindowButtons::CLOSE
                } else {
                    WindowButtons::CLOSE | WindowButtons::MINIMIZE
                })
        })
        .select()?;
    Ok(WindowFactory(creating_panel))
}

impl WindowFactory {
    pub fn create(&self, is_panel: bool) -> Result<AppWindow, slint::PlatformError> {
        // hook 在创建 adapter 时运行，早于应用标题赋值，不能通过 title 判断窗口角色。
        self.0.set(is_panel);
        let result = AppWindow::new();
        if let Ok(window) = &result {
            window.set_is_panel(is_panel);
            let _ = window.window().has_winit_window();
        }
        self.0.set(false);
        result
    }
}

pub async fn initialize(toolbar: &AppWindow, panel: &AppWindow) -> Result<(), String> {
    let owner = toolbar
        .window()
        .winit_window()
        .await
        .map_err(|error| error.to_string())?;
    let child = panel
        .window()
        .winit_window()
        .await
        .map_err(|error| error.to_string())?;
    let owner_handle = hwnd(&owner)?;
    let child_handle = hwnd(&child)?;
    // 两者都是本线程创建的顶层窗口。设置 owner，不使用 WS_CHILD，浮层可超出工具条边界。
    unsafe {
        SetWindowLongPtrW(child_handle, GWLP_HWNDPARENT, owner_handle.0 as isize);
    }
    if unsafe { GetWindow(child_handle, GW_OWNER) }.ok() != Some(owner_handle) {
        return Err("无法关联工具条与功能浮层".into());
    }
    constrain_main(toolbar, true)
}

pub fn connect(window: &AppWindow, on_event: impl Fn(NativeEvent) + 'static) {
    let on_event = Rc::new(on_event);
    let painted = on_event.clone();
    let paint_window = window.as_weak();
    crate::performance::attach(window, move || {
        if let Some(window) = paint_window.upgrade()
            && window.get_is_panel()
            && window.get_panel_shown()
            && !window.get_panel_revealed()
            && let Some(frame) = panel_frame(&window)
        {
            painted(NativeEvent::PanelPainted(frame));
        }
    });
    update_animation_preference(window);
    record_monitor(window);
    let weak = window.as_weak();
    window.on_window_drag_requested(move || {
        if let Some(window) = weak.upgrade()
            && let Some(Err(error)) = window
                .window()
                .with_winit_window(|native| native.drag_window())
        {
            eprintln!("无法拖动工具条：{error}");
        }
    });
    let weak = window.as_weak();
    window.on_window_menu_requested(move |x, y| {
        if let Some(window) = weak.upgrade() {
            window.window().with_winit_window(|native| {
                native.show_window_menu(LogicalPosition::new(f64::from(x), f64::from(y)));
            });
        }
    });
    let weak = window.as_weak();
    window.window().on_winit_window_event(move |native, event| {
        if let Some(window) = weak.upgrade() {
            match event {
                WindowEvent::MouseInput { state, .. }
                    if *state == slint::winit_030::winit::event::ElementState::Pressed =>
                {
                    crate::performance::input(window.get_is_panel());
                }
                WindowEvent::KeyboardInput { event, .. } if event.state.is_pressed() => {
                    crate::performance::input(window.get_is_panel());
                }
                WindowEvent::Focused(active) => {
                    window.set_window_active(*active);
                    if *active {
                        update_animation_preference(&window);
                        record_monitor(&window);
                    } else {
                        on_event(NativeEvent::Deactivated);
                    }
                }
                WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                    window.set_window_scale(*scale_factor as f32);
                    if !window.get_is_panel() {
                        on_event(NativeEvent::GeometryChanged);
                    }
                }
                WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
                    window.set_window_scale(native.scale_factor());
                    // 浮层由主窗口定位；它自己的 Moved/Resized 不能再触发一轮定位。
                    if !window.get_is_panel() {
                        on_event(NativeEvent::GeometryChanged);
                    }
                }
                _ => {}
            }
        }
        EventResult::Propagate
    });
}

pub fn has_focus(window: &AppWindow) -> bool {
    window
        .window()
        .with_winit_window(Window::has_focus)
        .unwrap_or(false)
}

fn record_monitor(window: &AppWindow) {
    if !crate::performance::enabled() {
        return;
    }
    window.window().with_winit_window(|native| {
        crate::performance::monitor(
            window.get_is_panel(),
            native
                .current_monitor()
                .and_then(|monitor| monitor.refresh_rate_millihertz()),
            native.scale_factor(),
        );
    });
}

pub fn focus(window: &AppWindow) {
    window.window().with_winit_window(Window::focus_window);
}

pub fn constrain_main(window: &AppWindow, reset_layout: bool) -> Result<(), String> {
    window
        .window()
        .with_winit_window(|native| {
            if native.is_minimized() == Some(true) {
                return Ok(());
            }
            let window_mode = window.get_window_mode();
            let work = work_area(native)?;
            let scale = native.scale_factor();
            let margin = (8.0 * scale).round() as i32;
            let available_width = (work.right - work.left - 2 * margin).max(1) as u32;
            let available_height = (work.bottom - work.top - 2 * margin).max(1) as u32;
            window.set_main_min_width(660.0_f32.min(available_width as f32 / scale as f32));
            window.set_main_min_height(320.0_f32.min(available_height as f32 / scale as f32));
            if reset_layout {
                native.set_resizable(window_mode);
                native.set_enabled_buttons(if window_mode {
                    WindowButtons::CLOSE | WindowButtons::MINIMIZE | WindowButtons::MAXIMIZE
                } else {
                    WindowButtons::CLOSE | WindowButtons::MINIMIZE
                });
                if native.is_maximized() {
                    native.set_maximized(false);
                }
            } else if window_mode && native.is_maximized() {
                return Ok(());
            }
            let current_size = window.window().size();
            let size = if window_mode {
                if reset_layout {
                    slint::PhysicalSize::new(
                        (860.0 * scale).round() as u32,
                        (f64::from(window.get_window_content_height().clamp(320.0, 680.0)) * scale)
                            .round() as u32,
                    )
                } else {
                    current_size
                }
            } else {
                slint::PhysicalSize::new(
                    (688.0 * scale).round() as u32,
                    (56.0 * scale).round() as u32,
                )
            };
            let size = slint::PhysicalSize::new(
                size.width.clamp(1, available_width),
                size.height.clamp(1, available_height),
            );
            if !window_mode {
                window.set_bar_width(size.width as f32 / scale as f32);
            }
            if size != current_size {
                window.window().set_size(size);
            }
            let current = native.outer_position().map_err(|error| error.to_string())?;
            let left_limit = work.left + margin;
            let top_limit = work.top + margin;
            let left = if reset_layout {
                (work.left + work.right - size.width as i32) / 2
            } else {
                current.x
            }
            .clamp(
                left_limit,
                (work.right - margin - size.width as i32).max(left_limit),
            );
            let top = if reset_layout {
                if window_mode {
                    (work.top + work.bottom - size.height as i32) / 2
                } else {
                    work.top + (24.0 * scale).round() as i32
                }
            } else {
                current.y
            }
            .clamp(
                top_limit,
                (work.bottom - margin - size.height as i32).max(top_limit),
            );
            if current.x != left || current.y != top {
                native.set_outer_position(PhysicalPosition::new(left, top));
            }
            Ok(())
        })
        .unwrap_or(Ok(()))
}

pub fn position_panel(owner: &AppWindow, panel: &AppWindow, kind: Panel) -> Result<(), String> {
    if kind == Panel::Hidden {
        return Ok(());
    }
    owner
        .window()
        .with_winit_window(|native| {
            if native.is_minimized() == Some(true) {
                return Ok(());
            }
            let work = work_area(native)?;
            let scale = native.scale_factor();
            let position = native.outer_position().map_err(|error| error.to_string())?;
            let size = native.inner_size();
            let owner_bounds = Bounds {
                left: position.x,
                top: position.y,
                right: position.x + size.width as i32,
                bottom: position.y + size.height as i32,
            };
            let gap = (8.0 * scale).round() as i32;
            let logical_width = panel
                .get_desired_panel_width()
                .min((work.right - work.left - 2 * gap).max(1) as f32 / scale as f32);
            let physical_width = (f64::from(logical_width) * scale).round() as u32;
            // 先给文本真实的可用宽度，再测量换行后的内容高度，避免估算字段数量导致空白或截断。
            // 使用像素取整后的宽度，避免非整数 DPI 下测量与定位来回改变换行。
            panel.set_panel_width(physical_width as f32 / scale as f32);
            let desired = (
                physical_width,
                (f64::from(panel.get_desired_panel_height().max(160.0)) * scale).ceil() as u32,
            );
            let bounds = if owner.get_window_mode() {
                place_window_panel(owner_bounds, work, desired, scale)
            } else {
                let anchor = match kind {
                    Panel::Library => Anchor::Start,
                    Panel::Details | Panel::Status | Panel::Appearance => Anchor::End,
                    _ => Anchor::Center,
                };
                place_panel(owner_bounds, work, desired, scale, anchor)
            };
            // 工作条上方展开时反转轻移方向；动画不逐帧移动 Win32 窗口。
            panel.set_panel_enter_offset(
                if owner.get_window_mode() || bounds.top >= owner_bounds.bottom {
                    8.0
                } else {
                    -8.0
                },
            );
            let size = slint::PhysicalSize::new(
                (bounds.right - bounds.left) as u32,
                (bounds.bottom - bounds.top) as u32,
            );
            panel.set_panel_width(size.width as f32 / scale as f32);
            panel.set_panel_height(size.height as f32 / scale as f32);
            // 定位到主窗口所在显示器后再应用该显示器的物理尺寸。
            if panel.window().position() != slint::PhysicalPosition::new(bounds.left, bounds.top) {
                panel
                    .window()
                    .set_position(slint::PhysicalPosition::new(bounds.left, bounds.top));
            }
            if panel.window().size() != size {
                panel.window().set_size(size);
            }
            Ok(())
        })
        .unwrap_or(Ok(()))
}

fn hwnd(window: &Window) -> Result<HWND, String> {
    match window
        .window_handle()
        .map_err(|error| error.to_string())?
        .as_raw()
    {
        RawWindowHandle::Win32(handle) => Ok(HWND(handle.hwnd.get() as *mut core::ffi::c_void)),
        _ => Err("没有获得 Windows 窗口句柄".into()),
    }
}

fn work_area(window: &Window) -> Result<Bounds, String> {
    let handle = hwnd(window)?;
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // MONITORINFO 是栈上缓冲区；HMONITOR 为借用值，无需关闭。
    if !unsafe {
        GetMonitorInfoW(
            MonitorFromWindow(handle, MONITOR_DEFAULTTONEAREST),
            &mut info,
        )
    }
    .as_bool()
    {
        return Err("无法读取工具条所在显示器的工作区域".into());
    }
    Ok(Bounds {
        left: info.rcWork.left,
        top: info.rcWork.top,
        right: info.rcWork.right,
        bottom: info.rcWork.bottom,
    })
}

fn update_animation_preference(window: &AppWindow) {
    let mut enabled = 0_i32;
    let result = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(std::ptr::from_mut(&mut enabled).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    window.set_animations_enabled(
        result.is_ok() && enabled != 0 && !crate::performance::animations_disabled(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::{LogicalSize, PhysicalSize};

    #[test]
    fn panel_waits_for_both_render_surface_and_native_resize() {
        let target = LogicalSize::new(460.0, 202.0);
        let library = PhysicalSize::new(690, 303);
        let previous_editor = PhysicalSize::new(990, 966);
        // Windows 和 Slint 可以先后完成；任一端还在旧尺寸都不能开始淡入。
        assert_eq!(
            PanelFrame::ready(1, target, 1.5, previous_editor, library),
            None
        );
        assert_eq!(
            PanelFrame::ready(1, target, 1.5, library, previous_editor),
            None
        );
        assert!(PanelFrame::ready(1, target, 1.5, library, library).is_some());
    }

    #[test]
    fn queued_panel_frame_is_invalidated_by_content_or_size_change() {
        let size = PhysicalSize::new(690, 303);
        let target = LogicalSize::from_physical(size, 1.5);
        let library = PanelFrame::ready(1, target, 1.5, size, size).unwrap();
        let details = PanelFrame::ready(3, target, 1.5, size, size).unwrap();
        assert_ne!(library, details);
        let taller = PhysicalSize::new(690, 420);
        let resized = PanelFrame::ready(
            1,
            LogicalSize::from_physical(taller, 1.5),
            1.5,
            taller,
            taller,
        )
        .unwrap();
        assert_ne!(library, resized);
    }

    #[test]
    fn panel_frame_preserves_pixel_rounding_at_fractional_dpi() {
        for scale in [1.0, 1.25, 1.5, 1.75, 2.25] {
            let size = PhysicalSize::new(577, 253);
            let target = LogicalSize::from_physical(size, scale as f32);
            assert!(PanelFrame::ready(1, target, scale, size, size).is_some());
        }
        let zero = PhysicalSize::new(0, 0);
        assert_eq!(
            PanelFrame::ready(1, LogicalSize::default(), 1.5, zero, zero),
            None
        );
    }
}
