#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Copy)]
pub enum Anchor {
    Start,
    Center,
    End,
}

/// 普通窗口上的面板居中放置，按整个工作区限高，不能按窗口外的剩余空间计算。
pub fn place_window_panel(owner: Bounds, work: Bounds, desired: (u32, u32), scale: f64) -> Bounds {
    let gap = (8.0 * scale).round() as i32;
    let left_limit = work.left + gap;
    let top_limit = work.top + gap;
    let right_limit = (work.right - gap).max(left_limit + 1);
    let bottom_limit = (work.bottom - gap).max(top_limit + 1);
    let width = (desired.0 as i32).clamp(1, right_limit - left_limit);
    let height = (desired.1 as i32).clamp(1, bottom_limit - top_limit);
    let left = ((owner.left + owner.right - width) / 2).clamp(left_limit, right_limit - width);
    let top = ((owner.top + owner.bottom - height) / 2).clamp(top_limit, bottom_limit - height);
    Bounds {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

/// 输入和输出均为屏幕物理像素；只将间距从逻辑像素换算一次。
pub fn place_panel(
    toolbar: Bounds,
    work: Bounds,
    desired: (u32, u32),
    scale: f64,
    anchor: Anchor,
) -> Bounds {
    let gap = (8.0 * scale).round() as i32;
    let left_limit = work.left + gap;
    let top_limit = work.top + gap;
    let right_limit = (work.right - gap).max(left_limit + 1);
    let bottom_limit = (work.bottom - gap).max(top_limit + 1);
    let width = (desired.0 as i32).clamp(1, right_limit - left_limit);
    let below = (bottom_limit - toolbar.bottom - gap).max(0);
    let above = (toolbar.top - gap - top_limit).max(0);
    let open_below = below >= desired.1 as i32 || below >= above;
    let available = if open_below { below } else { above };
    let height = (desired.1 as i32).clamp(1, available.max(1).min(bottom_limit - top_limit));
    let anchor_x = match anchor {
        Anchor::Start => toolbar.left + (44.0 * scale).round() as i32,
        Anchor::Center => (toolbar.left + toolbar.right - width) / 2,
        Anchor::End => toolbar.right - (80.0 * scale).round() as i32 - width,
    };
    let left = anchor_x.clamp(left_limit, right_limit - width);
    let top = if open_below {
        toolbar.bottom + gap
    } else {
        toolbar.top - gap - height
    }
    .clamp(top_limit, bottom_limit - height);
    Bounds {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(left: i32, top: i32, width: i32, height: i32) -> Bounds {
        Bounds {
            left,
            top,
            right: left + width,
            bottom: top + height,
        }
    }

    #[test]
    fn panel_opens_below_without_resizing_toolbar() {
        let toolbar = bounds(600, 24, 720, 56);
        let panel = place_panel(
            toolbar,
            bounds(0, 0, 1920, 1040),
            (480, 400),
            1.0,
            Anchor::Start,
        );
        assert_eq!(panel, bounds(644, 88, 480, 400));
        assert!(panel.top > toolbar.bottom);
    }

    #[test]
    fn bottom_edge_opens_above() {
        let toolbar = bounds(1000, 970, 720, 56);
        let panel = place_panel(
            toolbar,
            bounds(0, 0, 1920, 1040),
            (480, 400),
            1.0,
            Anchor::End,
        );
        assert_eq!(panel.bottom, toolbar.top - 8);
        assert_eq!(panel.bottom - panel.top, 400);
    }

    #[test]
    fn panel_stays_on_negative_coordinate_monitor() {
        let work = bounds(-1920, -120, 1920, 1040);
        let panel = place_panel(
            bounds(-100, 100, 720, 56),
            work,
            (680, 640),
            1.0,
            Anchor::Center,
        );
        assert_eq!(panel.right, -8);
        assert!(panel.left >= work.left + 8 && panel.bottom <= work.bottom - 8);
    }

    #[test]
    fn dpi_scales_spacing_once_and_clamps_editor_to_available_space() {
        let toolbar = bounds(200, 48, 1440, 112);
        let panel = place_panel(
            toolbar,
            bounds(0, 0, 1920, 1040),
            (1360, 1280),
            2.0,
            Anchor::Center,
        );
        assert_eq!(panel.top, 176);
        assert_eq!(panel.bottom, 1024);
        assert_eq!(panel.right - panel.left, 1360);
    }

    #[test]
    fn small_work_area_clamps_width_and_keeps_toolbar_accessible() {
        let toolbar = bounds(0, 100, 720, 56);
        let panel = place_panel(
            toolbar,
            bounds(0, 0, 600, 400),
            (680, 640),
            1.0,
            Anchor::Center,
        );
        assert_eq!(panel.left, 8);
        assert_eq!(panel.right, 592);
        assert!(panel.top > toolbar.bottom);
        assert_eq!(panel.bottom, 392);
    }

    #[test]
    fn window_panel_keeps_its_content_height_when_owner_is_maximized() {
        let work = bounds(0, 0, 1920, 1040);
        let panel = place_window_panel(work, work, (660, 720), 1.0);
        assert_eq!(panel, bounds(630, 160, 660, 720));
    }

    #[test]
    fn window_panel_clamps_tall_content_on_a_scaled_negative_monitor() {
        let work = bounds(-1200, -100, 1200, 900);
        let panel = place_window_panel(bounds(-350, 600, 1000, 720), work, (1320, 1600), 2.0);
        assert_eq!(panel, bounds(-1184, -84, 1168, 868));
    }
}
