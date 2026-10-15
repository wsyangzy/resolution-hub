use std::{cell::RefCell, collections::HashMap, io::Cursor, sync::mpsc};

use resolution_hub_lib::{
    icon,
    model::{ApplicationIcon, ApplicationIconRequest, GameProfile, LaunchType},
};
use slint::{ComponentHandle, Model, ModelRc, SharedPixelBuffer};

use crate::{AppWindow, GameEntry, SourceEntry};

const ICON_SIZE: u32 = 64;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key([String; 4]);

impl From<&ApplicationIconRequest> for Key {
    fn from(request: &ApplicationIconRequest) -> Self {
        Self([
            request.executable.trim().to_lowercase(),
            request.shortcut_path.trim().to_lowercase(),
            request.steam_app_id.trim().to_string(),
            request.process_name.trim().to_lowercase(),
        ])
    }
}

#[derive(Default)]
struct Cache {
    ids: HashMap<Key, i32>,
    entries: Vec<CachedIcon>,
}

struct CachedIcon {
    request: ApplicationIconRequest,
    image: slint::Image,
    requested: bool,
}

impl Cache {
    fn register(&mut self, request: ApplicationIconRequest) -> (i32, slint::Image) {
        let key = Key::from(&request);
        if let Some(id) = self.ids.get(&key) {
            return (*id, self.entries[(*id - 1) as usize].image.clone());
        }
        let id = self.entries.len() as i32 + 1;
        self.ids.insert(key, id);
        self.entries.push(CachedIcon {
            request,
            image: slint::Image::default(),
            requested: false,
        });
        (id, slint::Image::default())
    }

    fn prepare(&mut self, id: i32) -> Option<ApplicationIconRequest> {
        let entry = self.entries.get_mut(usize::try_from(id - 1).ok()?)?;
        if entry.requested {
            return None;
        }
        // 成功和失败都缓存；缺失的快捷方式不会在每次悬停或输入时重复访问磁盘。
        entry.requested = true;
        Some(entry.request.clone())
    }
}

// Slint Image 只在事件循环线程创建和使用，后台只传递尺寸与像素。
thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
    static TARGETS: RefCell<Targets> = RefCell::new(Targets::default());
}

struct IndexedModel<T> {
    model: ModelRc<T>,
    rows: HashMap<i32, Vec<usize>>,
}

impl<T: Clone + 'static> IndexedModel<T> {
    fn new(model: ModelRc<T>, icon_id: impl Fn(&T) -> i32) -> Self {
        let mut rows: HashMap<i32, Vec<usize>> = HashMap::new();
        for index in 0..model.row_count() {
            if let Some(entry) = model.row_data(index) {
                rows.entry(icon_id(&entry)).or_default().push(index);
            }
        }
        Self { model, rows }
    }

    fn update(&self, id: i32, mut update: impl FnMut(&mut T) -> bool) {
        if let Some(rows) = self.rows.get(&id) {
            for &index in rows {
                if let Some(mut entry) = self.model.row_data(index)
                    && update(&mut entry)
                {
                    self.model.set_row_data(index, entry);
                }
            }
        }
    }
}

#[derive(Default)]
struct Targets {
    games: Option<IndexedModel<GameEntry>>,
    sources: Option<IndexedModel<SourceEntry>>,
    candidates: Option<IndexedModel<SourceEntry>>,
}

pub fn track_games(model: ModelRc<GameEntry>) {
    TARGETS.with_borrow_mut(|targets| {
        targets.games = Some(IndexedModel::new(model, |entry| entry.icon_id))
    });
}

pub fn track_sources(model: ModelRc<SourceEntry>) {
    TARGETS.with_borrow_mut(|targets| {
        targets.sources = Some(IndexedModel::new(model, |entry| entry.icon_id))
    });
}

pub fn track_candidates(model: ModelRc<SourceEntry>) {
    TARGETS.with_borrow_mut(|targets| {
        targets.candidates = Some(IndexedModel::new(model, |entry| entry.icon_id))
    });
}

pub fn register(request: ApplicationIconRequest) -> (i32, slint::Image) {
    CACHE.with_borrow_mut(|cache| cache.register(request))
}

pub fn for_profile(profile: &GameProfile) -> (i32, slint::Image) {
    register(profile_request(profile))
}

fn profile_request(profile: &GameProfile) -> ApplicationIconRequest {
    // 与原版 profileIconSource 一致，旧配置中其他来源的残留路径不能覆盖当前游戏。
    let mut request = ApplicationIconRequest {
        process_name: profile.process_name.clone(),
        ..Default::default()
    };
    match profile.launch_type {
        LaunchType::Executable => request.executable = profile.executable.clone(),
        LaunchType::Steam => request.steam_app_id = profile.steam_app_id.clone(),
        LaunchType::Shortcut => request.shortcut_path = profile.shortcut_path.clone(),
    }
    request
}

#[derive(Clone)]
pub struct Loader(mpsc::Sender<(i32, ApplicationIconRequest)>);

impl Loader {
    pub fn start(window: &AppWindow, panel: &AppWindow) -> Result<Self, std::io::Error> {
        let (sender, receiver) = mpsc::channel::<(i32, ApplicationIconRequest)>();
        let windows = [window.as_weak(), panel.as_weak()];
        std::thread::Builder::new()
            .name("native-game-icons".into())
            .spawn(move || {
                while let Ok((id, request)) = receiver.recv() {
                    let result = load_pixels(&request, icon::load);
                    let Some(pixels) = result else { continue };
                    let windows = windows.clone();
                    if slint::invoke_from_event_loop(move || {
                        let buffer = SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                            &pixels.rgba,
                            pixels.width,
                            pixels.height,
                        );
                        let image = if pixels.premultiplied {
                            slint::Image::from_rgba8_premultiplied(buffer)
                        } else {
                            slint::Image::from_rgba8(buffer)
                        };
                        CACHE.with_borrow_mut(|cache| {
                            if let Some(entry) = cache.entries.get_mut((id - 1) as usize) {
                                entry.image = image.clone();
                            }
                        });
                        // 当前三个模型各有一份索引，共享游戏模型只更新一次；复杂度只随匹配行数增长。
                        TARGETS.with_borrow(|targets| {
                            if let Some(games) = &targets.games {
                                games.update(id, |entry| replace_icon(&mut entry.icon, &image));
                            }
                            for sources in [&targets.sources, &targets.candidates]
                                .into_iter()
                                .flatten()
                            {
                                sources.update(id, |entry| replace_icon(&mut entry.icon, &image));
                            }
                        });
                        for window in windows.iter().filter_map(slint::Weak::upgrade) {
                            if window.get_profile_icon_id() == id {
                                window.set_profile_icon(image.clone());
                            }
                        }
                    })
                    .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self(sender))
    }

    pub fn request(&self, id: i32) {
        if id <= 0 {
            return;
        }
        if let Some(request) = CACHE.with_borrow_mut(|cache| cache.prepare(id)) {
            let _ = self.0.send((id, request));
        }
    }
}

fn replace_icon(current: &mut slint::Image, image: &slint::Image) -> bool {
    if current == image {
        return false;
    }
    *current = image.clone();
    true
}

struct Pixels {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    premultiplied: bool,
}

fn load_pixels(
    request: &ApplicationIconRequest,
    load: impl FnOnce(&ApplicationIconRequest) -> Option<ApplicationIcon>,
) -> Option<Pixels> {
    // 保留完整主进程名，让共享加载器优先提取游戏 EXE 的透明图标，最后回退 Steam 缓存。
    // 文件查找和解码均在后台执行，不以带底色的缓存图替代原版图标来节省 UI 线程时间。
    load(request).and_then(decode)
}

fn decode(icon: ApplicationIcon) -> Option<Pixels> {
    let premultiplied = icon.kind == "rgba";
    let image = if premultiplied {
        if icon.width == 0 || icon.height == 0 || icon.width > 512 || icon.height > 512 {
            return None;
        }
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_raw(
            icon.width,
            icon.height,
            icon.data,
        )?)
    } else if icon.kind == "encoded" && icon.data.len() <= 1024 * 1024 {
        let mut reader = image::ImageReader::new(Cursor::new(icon.data))
            .with_guessed_format()
            .ok()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        reader.decode().ok()?
    } else {
        return None;
    };
    let image = if image.width() > ICON_SIZE || image.height() > ICON_SIZE {
        image.thumbnail(ICON_SIZE, ICON_SIZE)
    } else {
        image
    }
    .to_rgba8();
    Some(Pixels {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
        premultiplied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_completion_only_visits_matching_rows_in_a_large_catalog() {
        let model = ModelRc::new(slint::VecModel::from(
            (0..4096)
                .map(|index| SourceEntry {
                    icon_id: index % 1024,
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        ));
        let indexed = IndexedModel::new(model.clone(), |entry| entry.icon_id);
        let mut visits = 0;
        indexed.update(25, |entry| {
            visits += 1;
            entry.title = "loaded".into();
            true
        });
        assert_eq!(visits, 4);
        for index in 0..4096 {
            let entry = model.row_data(index).unwrap();
            assert_eq!(entry.title == "loaded", index % 1024 == 25);
        }
        indexed.update(9999, |_| panic!("unknown icon must not visit any row"));
    }

    #[test]
    fn replacing_the_model_drops_old_row_targets() {
        let old = ModelRc::new(slint::VecModel::from(vec![SourceEntry {
            icon_id: 7,
            ..Default::default()
        }]));
        let new = ModelRc::new(slint::VecModel::from(vec![SourceEntry {
            icon_id: 9,
            ..Default::default()
        }]));
        let mut targets = Targets {
            sources: Some(IndexedModel::new(old.clone(), |entry| entry.icon_id)),
            ..Default::default()
        };
        targets.sources = Some(IndexedModel::new(new.clone(), |entry| entry.icon_id));
        targets
            .sources
            .as_ref()
            .unwrap()
            .update(7, |_| panic!("old model must not be updated"));
        targets.sources.as_ref().unwrap().update(9, |entry| {
            entry.title = "new".into();
            true
        });
        assert!(old.row_data(0).unwrap().title.is_empty());
        assert_eq!(new.row_data(0).unwrap().title, "new");
    }

    fn profile(launch_type: LaunchType) -> GameProfile {
        let mut profile: GameProfile = serde_json::from_value(serde_json::json!({
            "id": "icon-source",
            "name": "Game",
            "launchType": "steam",
            "executable": "C:\\Games\\OldGame.exe",
            "arguments": "",
            "steamAppId": "824270",
            "shortcutPath": "C:\\Games\\OldLauncher.lnk",
            "processName": "FPSAimTrainer-Win64-Shipping.exe",
            "displayDevice": "DISPLAY1",
            "width": 1920,
            "height": 1080,
            "refreshRate": 144,
            "restoreDelayMs": 2000
        }))
        .unwrap();
        profile.launch_type = launch_type;
        profile
    }

    #[test]
    fn profile_icons_ignore_paths_from_other_launch_types() {
        for (launch_type, expected) in [
            (LaunchType::Steam, ["", "", "824270"]),
            (LaunchType::Executable, ["C:\\Games\\OldGame.exe", "", ""]),
            (LaunchType::Shortcut, ["", "C:\\Games\\OldLauncher.lnk", ""]),
        ] {
            let profile = profile(launch_type);
            let request = profile_request(&profile);
            assert_eq!(
                [
                    request.executable.as_str(),
                    request.shortcut_path.as_str(),
                    request.steam_app_id.as_str(),
                ],
                expected
            );
            assert_eq!(request.process_name, profile.process_name);
        }
    }

    #[test]
    fn background_loader_preserves_game_process_and_transparent_pixels() {
        let request = profile_request(&profile(LaunchType::Steam));
        let pixels = load_pixels(&request, |request| {
            // 主进程名为空时，共享加载器只能回退缓存图，无法找到 EXE 图标。
            assert_eq!(request.steam_app_id, "824270");
            assert_eq!(request.process_name, "FPSAimTrainer-Win64-Shipping.exe");
            Some(ApplicationIcon {
                kind: "rgba".into(),
                mime_type: String::new(),
                width: 2,
                height: 1,
                data: vec![0, 0, 0, 0, 0, 0, 0, 255],
            })
        })
        .unwrap();
        // 透明背景与真正的黑色内容都应保留，不能用颜色抠图代替 alpha。
        assert_eq!(pixels.rgba, [0, 0, 0, 0, 0, 0, 0, 255]);
        assert!(pixels.premultiplied);
    }

    #[test]
    fn equivalent_windows_paths_share_one_request_including_failed_loads() {
        let mut cache = Cache::default();
        let (first, _) = cache.register(ApplicationIconRequest {
            executable: " C:\\Games\\Game.exe ".into(),
            ..Default::default()
        });
        let (second, _) = cache.register(ApplicationIconRequest {
            executable: "c:\\games\\game.EXE".into(),
            ..Default::default()
        });
        assert_eq!(first, second);
        assert!(cache.prepare(first).is_some());
        assert!(cache.prepare(second).is_none());
        assert!(cache.prepare(0).is_none());
    }

    #[test]
    fn raw_pixels_preserve_windows_alpha_and_reject_invalid_buffers() {
        let icon = ApplicationIcon {
            kind: "rgba".into(),
            mime_type: String::new(),
            width: 1,
            height: 1,
            data: vec![32, 16, 8, 64],
        };
        let decoded = decode(icon.clone()).unwrap();
        assert_eq!(decoded.rgba, [32, 16, 8, 64]);
        assert!(decoded.premultiplied);
        assert!(
            decode(ApplicationIcon {
                width: 1024,
                ..icon.clone()
            })
            .is_none()
        );
        assert!(
            decode(ApplicationIcon {
                data: vec![0; 3],
                ..icon
            })
            .is_none()
        );
    }

    #[test]
    fn encoded_icons_are_decoded_and_downscaled_off_the_ui_thread() {
        let image = image::DynamicImage::new_rgba8(256, 128);
        let mut encoded = Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let icon = ApplicationIcon {
            kind: "encoded".into(),
            mime_type: "image/png".into(),
            width: 0,
            height: 0,
            data: encoded.into_inner(),
        };
        let decoded = decode(icon.clone()).unwrap();
        assert_eq!((decoded.width, decoded.height), (64, 32));
        assert!(!decoded.premultiplied);
        assert!(
            decode(ApplicationIcon {
                data: vec![0; 8],
                ..icon
            })
            .is_none()
        );
    }
}
