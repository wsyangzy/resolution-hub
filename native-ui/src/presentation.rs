use std::cell::RefCell;

use resolution_hub_core::model::ApplicationIconRequest;
use slint::{Model, ModelRc, VecModel};

use crate::{AppWindow, GameEntry, SourceEntry, editor::EditorState, icons, state::AppState};

#[derive(Default)]
struct Models {
    profiles_revision: Option<u64>,
    profiles: ModelRc<GameEntry>,
    displays_revision: Option<u64>,
    modes_key: Option<(u64, i32)>,
    sources_key: Option<SourceKey>,
    candidates_key: Vec<(u32, String, String)>,
}

#[derive(PartialEq)]
struct SourceKey {
    revision: u64,
    kind: i32,
    search: String,
    selected: Option<String>,
}

thread_local! {
    static MODELS: RefCell<Models> = RefCell::new(Models::default());
}

pub fn profiles(window: &AppWindow, state: &AppState) {
    MODELS.with_borrow_mut(|models| {
        if models.profiles_revision != Some(state.profiles_revision) {
            let entries: Vec<GameEntry> = state
                .profiles
                .iter()
                .map(|profile| {
                    let (icon_id, icon) = icons::for_profile(profile);
                    GameEntry {
                        id: profile.id.clone().into(),
                        name: profile.name.clone().into(),
                        resolution: format!("{} × {}", profile.width, profile.height).into(),
                        refresh: format!("{} Hz", profile.refresh_rate).into(),
                        initial: profile
                            .name
                            .trim()
                            .chars()
                            .next()
                            .map(|initial| initial.to_uppercase().to_string())
                            .unwrap_or_else(|| "游".into())
                            .into(),
                        icon_id,
                        icon,
                    }
                })
                .collect();
            models.profiles = ModelRc::new(VecModel::from(entries));
            icons::track_games(models.profiles.clone());
            models.profiles_revision = Some(state.profiles_revision);
        }
        if window.get_profile_entries() != models.profiles {
            window.set_profile_entries(models.profiles.clone());
        }
        let selected = state
            .selected_index()
            .and_then(|index| models.profiles.row_data(index))
            .unwrap_or_default();
        window.set_profile_icon_id(selected.icon_id);
        window.set_profile_icon(selected.icon);
        window.set_profile_initial(selected.initial);
        // 新表单的 revision 会重新计数；在关闭后失效缓存，避免再次打开时复用旧目录。
        if state.editor.is_none() {
            models.displays_revision = None;
            models.modes_key = None;
            models.sources_key = None;
            models.candidates_key.clear();
        }
    });
}

pub fn editor_models(window: &AppWindow, editor: &EditorState) {
    MODELS.with_borrow_mut(|models| {
        if models.displays_revision != Some(editor.catalog_revision) {
            let entries: Vec<slint::SharedString> = editor
                .catalog
                .as_ref()
                .map(|catalog| {
                    catalog
                        .displays
                        .iter()
                        .map(|choice| {
                            format!(
                                "{}{}",
                                choice.info.friendly_name,
                                if choice.info.is_primary {
                                    "（主显示器）"
                                } else {
                                    ""
                                }
                            )
                            .into()
                        })
                        .collect()
                })
                .unwrap_or_default();
            window.set_editor_displays(ModelRc::new(VecModel::from(entries)));
            models.displays_revision = Some(editor.catalog_revision);
        }
        let modes_key = (editor.catalog_revision, editor.draft.display_index);
        if models.modes_key != Some(modes_key) {
            let entries: Vec<slint::SharedString> = editor
                .selected_display()
                .map(|choice| {
                    choice
                        .modes
                        .iter()
                        .map(|mode| {
                            format!(
                                "{} × {} · {} Hz",
                                mode.width, mode.height, mode.refresh_rate
                            )
                            .into()
                        })
                        .collect()
                })
                .unwrap_or_default();
            window.set_editor_modes(ModelRc::new(VecModel::from(entries)));
            models.modes_key = Some(modes_key);
        }
        let sources_key = SourceKey {
            revision: editor.catalog_revision,
            kind: editor.draft.source_kind,
            search: editor.steam_search.trim().to_lowercase(),
            selected: editor
                .selected_source
                .as_ref()
                .map(|source| source.key.clone()),
        };
        if models.sources_key.as_ref() != Some(&sources_key) {
            let entries: Vec<SourceEntry> = editor
                .source_entries()
                .into_iter()
                .map(|mut entry| {
                    if let Some(request) = editor.source_icon_request(entry.index as usize) {
                        (entry.icon_id, entry.icon) = icons::register(request);
                    }
                    entry
                })
                .collect();
            let model = ModelRc::new(VecModel::from(entries));
            icons::track_sources(model.clone());
            window.set_editor_source_entries(model);
            window.set_editor_source_notice(editor.source_notice().into());
            models.sources_key = Some(sources_key);
        }
        let candidates_key: Vec<_> = editor
            .candidates
            .iter()
            .map(|app| (app.pid, app.process_name.clone(), app.executable.clone()))
            .collect();
        if models.candidates_key != candidates_key
            || window.get_editor_candidates().row_count() != candidates_key.len()
        {
            let entries: Vec<SourceEntry> = editor
                .candidates
                .iter()
                .enumerate()
                .map(|(index, app)| {
                    let (icon_id, icon) = icons::register(ApplicationIconRequest {
                        executable: app.executable.clone(),
                        ..Default::default()
                    });
                    SourceEntry {
                        index: index as i32,
                        title: app.process_name.clone().into(),
                        detail: app.executable.clone().into(),
                        meta: format!("PID {}", app.pid).into(),
                        selected: false,
                        icon_id,
                        icon,
                    }
                })
                .collect();
            let model = ModelRc::new(VecModel::from(entries));
            icons::track_candidates(model.clone());
            window.set_editor_candidates(model);
            models.candidates_key = candidates_key;
        }
    });
}
