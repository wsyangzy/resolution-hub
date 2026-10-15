use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ViewMode {
    #[default]
    Toolbar,
    Window,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    #[serde(default)]
    pub view_mode: ViewMode,
}

fn path() -> Result<PathBuf, String> {
    let base = std::env::var_os("APPDATA").ok_or("无法定位 APPDATA 目录")?;
    Ok(PathBuf::from(base)
        .join("resolution-hub")
        .join("native-ui-settings.json"))
}

pub fn load() -> Result<Preferences, String> {
    load_from(&path()?)
}

fn load_from(path: &Path) -> Result<Preferences, String> {
    match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(content.trim_start_matches('\u{feff}'))
            .map_err(|error| format!("无法读取原生界面偏好：{error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Preferences::default()),
        Err(error) => Err(format!("无法读取原生界面偏好：{error}")),
    }
}

pub fn save(mode: ViewMode) -> Result<(), String> {
    save_to(&path()?, mode)
}

enum WriteRequest {
    Save(ViewMode),
    Flush(mpsc::Sender<()>),
}

#[derive(Clone)]
pub struct Writer(mpsc::Sender<WriteRequest>);

impl Writer {
    pub fn start(
        report: impl Fn(ViewMode, Result<(), String>) + Send + 'static,
    ) -> Result<Self, std::io::Error> {
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("native-ui-preferences".into())
            .spawn(move || {
                // 单一写入者严格按点击顺序保存，不让较早的后台任务覆盖较晚的选择。
                while let Ok(request) = receiver.recv() {
                    match request {
                        WriteRequest::Save(mode) => report(mode, save(mode)),
                        WriteRequest::Flush(done) => {
                            let _ = done.send(());
                        }
                    }
                }
            })?;
        Ok(Self(sender))
    }

    pub fn save(&self, mode: ViewMode) -> Result<(), String> {
        self.0
            .send(WriteRequest::Save(mode))
            .map_err(|_| "无法保存界面布局：后台任务已停止".into())
    }

    pub fn flush(&self) {
        let (sender, receiver) = mpsc::channel();
        if self.0.send(WriteRequest::Flush(sender)).is_ok() {
            let _ = receiver.recv_timeout(Duration::from_secs(2));
        }
    }
}

fn save_to(path: &Path, mode: ViewMode) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法创建界面偏好目录：{error}"))?;
    }
    let content = serde_json::to_vec_pretty(&Preferences { view_mode: mode })
        .map_err(|error| format!("无法生成界面偏好：{error}"))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, content).map_err(|error| format!("无法保存界面偏好：{error}"))?;
    // Windows 的 rename 会替换目标文件；调用方串行保存，避免快速切换时旧偏好覆盖新偏好。
    fs::rename(&temporary, path).map_err(|error| format!("无法替换界面偏好：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_view_mode_defaults_to_toolbar() {
        let preferences: Preferences = serde_json::from_str(r#"{"futureOption":true}"#).unwrap();
        assert_eq!(preferences.view_mode, ViewMode::Toolbar);
        assert_eq!(
            serde_json::to_string(&Preferences {
                view_mode: ViewMode::Window
            })
            .unwrap(),
            r#"{"viewMode":"window"}"#
        );
    }

    #[test]
    fn saves_and_replaces_preference_in_an_isolated_directory() {
        let directory = std::env::temp_dir().join(format!(
            "resolution-hub-view-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let file = directory.join("native-ui-settings.json");
        assert_eq!(load_from(&file).unwrap().view_mode, ViewMode::Toolbar);
        save_to(&file, ViewMode::Window).unwrap();
        assert_eq!(load_from(&file).unwrap().view_mode, ViewMode::Window);
        save_to(&file, ViewMode::Toolbar).unwrap();
        assert_eq!(load_from(&file).unwrap().view_mode, ViewMode::Toolbar);
        fs::write(&file, "invalid-json").unwrap();
        assert!(load_from(&file).is_err());
        fs::remove_file(&file).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
