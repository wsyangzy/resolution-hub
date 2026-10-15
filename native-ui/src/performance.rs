use std::{
    fs::File,
    io::{BufWriter, Write},
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use slint::ComponentHandle;

use crate::AppWindow;

struct Recorder {
    started: Instant,
    sender: mpsc::SyncSender<serde_json::Value>,
    dropped: AtomicU64,
}

static RECORDER: OnceLock<Recorder> = OnceLock::new();

/// 显式开启才创建日志线程、有界队列和帧采样，常态只有浮层所需的绘制完成通知。
pub fn initialize() -> Result<(), std::io::Error> {
    let Some(path) = std::env::var_os("RESOLUTION_HUB_PERF_LOG") else {
        return Ok(());
    };
    let file = File::create(path)?;
    let (sender, receiver) = mpsc::sync_channel::<serde_json::Value>(256);
    std::thread::Builder::new()
        .name("native-performance-log".into())
        .spawn(move || {
            let mut writer = BufWriter::new(file);
            let mut bytes = 0;
            while let Ok(event) = receiver.recv() {
                // 一次会话最多写入 8 MiB；拥塞时丢弃诊断记录，绝不阻塞交互。
                let Ok(line) = serde_json::to_string(&event) else {
                    continue;
                };
                bytes += line.len() + 1;
                if bytes > 8 * 1024 * 1024 {
                    break;
                }
                if writeln!(writer, "{line}")
                    .and_then(|_| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        })?;
    let _ = RECORDER.set(Recorder {
        started: Instant::now(),
        sender,
        dropped: AtomicU64::new(0),
    });
    record(serde_json::json!({
        "kind": "session", "debugBuild": cfg!(debug_assertions),
        "animationsDisabled": animations_disabled(), "renderer": "femtovg"
    }));
    Ok(())
}

fn record(mut event: serde_json::Value) {
    if let Some(recorder) = RECORDER.get() {
        event["atMs"] = serde_json::json!(recorder.started.elapsed().as_secs_f64() * 1000.0);
        event["droppedRecords"] = serde_json::json!(recorder.dropped.load(Ordering::Relaxed));
        if recorder.sender.try_send(event).is_err() {
            recorder.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub fn animations_disabled() -> bool {
    std::env::var_os("RESOLUTION_HUB_DISABLE_ANIMATIONS").is_some_and(|value| value == "1")
}

pub fn enabled() -> bool {
    RECORDER.get().is_some()
}

pub fn monitor(is_panel: bool, refresh_millihertz: Option<u32>, scale: f64) {
    if RECORDER.get().is_none() {
        return;
    }
    record(serde_json::json!({
        "kind": "monitor", "window": if is_panel { "panel" } else { "main" },
        "refreshMillihertz": refresh_millihertz, "scaleFactor": scale,
        "expectedFrameMs": refresh_millihertz.filter(|rate| *rate > 0).map(|rate| 1_000_000.0 / f64::from(rate))
    }));
}

pub struct Timing {
    start: Option<Instant>,
    name: &'static str,
}

pub fn measure(name: &'static str) -> Timing {
    Timing {
        start: RECORDER.get().map(|_| Instant::now()),
        name,
    }
}

impl Drop for Timing {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            record(serde_json::json!({
                "kind": "work", "name": self.name,
                "durationMs": start.elapsed().as_secs_f64() * 1000.0
            }));
        }
    }
}

#[derive(Default)]
struct Frames {
    previous: Option<Instant>,
    rendering: Option<Instant>,
    input: Option<Instant>,
    animating: bool,
}

impl Frames {
    fn interval(&mut self, now: Instant) -> Option<Duration> {
        let previous = self.previous.replace(now)?;
        let interval = now.saturating_duration_since(previous);
        Some(interval)
    }
}

/// 记录提交绘制的间隔与 CPU 绘制段；AfterRendering 位于交换缓冲之前，并非 GPU 呈现时间。
pub fn attach(window: &AppWindow, mut after_render: impl FnMut() + 'static) {
    let diagnostics = RECORDER.get().is_some();
    let name = if window.get_is_panel() {
        "panel"
    } else {
        "main"
    };
    let frames = std::rc::Rc::new(std::cell::RefCell::new(Frames::default()));
    let render_frames = frames.clone();
    let frame_window = window.as_weak();
    let result = window.window().set_rendering_notifier(move |state, _| {
        if matches!(state, slint::RenderingState::AfterRendering) {
            after_render();
        }
        if !diagnostics {
            return;
        }
        let mut frames = render_frames.borrow_mut();
        match state {
            slint::RenderingState::BeforeRendering => {
                let now = Instant::now();
                let interval = frames.interval(now);
                let animating = frame_window.upgrade().is_some_and(|window| {
                    window.get_is_panel()
                        && window.get_panel_progress() > 0.0
                        && window.get_panel_progress() < 1.0
                });
                let animation_interval = frames.animating && animating;
                frames.animating = animating;
                frames.rendering = Some(now);
                let input = frames.input.take().map(|input| now.duration_since(input));
                record(serde_json::json!({
                    "kind": "frame", "window": name,
                    "intervalMs": interval.map(|value| value.as_secs_f64() * 1000.0),
                    "longGap": interval.is_some_and(|value| value >= Duration::from_millis(250)),
                    "animationInterval": animation_interval,
                    "inputToRenderMs": input.map(|value| value.as_secs_f64() * 1000.0)
                }));
            }
            slint::RenderingState::AfterRendering => {
                if let Some(start) = frames.rendering.take() {
                    record(serde_json::json!({
                        "kind": "render-cpu", "window": name,
                        "durationMs": start.elapsed().as_secs_f64() * 1000.0
                    }));
                }
            }
            _ => {}
        }
    });
    window.set_frame_notifier_ready(result.is_ok());
    if let Err(error) = result {
        record(
            serde_json::json!({"kind": "notifier-error", "window": name, "message": error.to_string()}),
        );
    }
    if diagnostics {
        INPUTS.with_borrow_mut(|inputs| inputs.push((name, frames)));
    }
}

type InputFrames = Vec<(&'static str, std::rc::Rc<std::cell::RefCell<Frames>>)>;
thread_local! { static INPUTS: std::cell::RefCell<InputFrames> = const { std::cell::RefCell::new(Vec::new()) }; }

pub fn input(is_panel: bool) {
    if RECORDER.get().is_none() {
        return;
    }
    let name = if is_panel { "panel" } else { "main" };
    INPUTS.with_borrow_mut(|inputs| {
        for (window, frames) in inputs {
            if *window == name {
                frames.borrow_mut().input.get_or_insert_with(Instant::now);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_intervals_keep_long_gaps_for_analysis_and_do_not_invent_a_first_interval() {
        let mut frames = Frames::default();
        let start = Instant::now();
        assert_eq!(frames.interval(start), None);
        assert_eq!(
            frames.interval(start + Duration::from_millis(8)),
            Some(Duration::from_millis(8))
        );
        assert_eq!(
            frames.interval(start + Duration::from_secs(5)),
            Some(Duration::from_millis(4992))
        );
        assert_eq!(
            frames.interval(start + Duration::from_millis(5016)),
            Some(Duration::from_millis(16))
        );
    }
}
