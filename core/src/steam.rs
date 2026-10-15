use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use keyvalues_parser::{Obj, Value};
use winreg::{RegKey, enums::HKEY_CURRENT_USER};

use crate::model::SteamGame;

pub fn list_installed_games() -> Result<Vec<SteamGame>, String> {
    let Some(steam_path) = locate_steam() else {
        return Ok(Vec::new());
    };

    let mut libraries = vec![steam_path.clone()];
    let library_file = steam_path.join("steamapps").join("libraryfolders.vdf");
    if let Ok(content) = fs::read_to_string(library_file) {
        libraries.extend(parse_library_paths(&content));
    }

    let mut games = Vec::new();
    let mut seen = HashSet::new();
    for library in libraries {
        let steamapps = library.join("steamapps");
        let Ok(entries) = fs::read_dir(&steamapps) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !filename.starts_with("appmanifest_") || !filename.ends_with(".acf") {
                continue;
            }
            if let Some(game) = parse_manifest(&path, &steamapps)
                && seen.insert(game.app_id.clone())
            {
                games.push(game);
            }
        }
    }

    games.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(games)
}

pub(crate) fn cached_icon(app_id: &str) -> Option<(String, Vec<u8>)> {
    if app_id.is_empty() || !app_id.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }

    let cache = locate_steam()?.join("appcache").join("librarycache");
    for extension in ["jpg", "png"] {
        let legacy = cache.join(format!("{app_id}_icon.{extension}"));
        if let Some(icon) = read_icon(&legacy) {
            return Some(icon);
        }
    }

    let directory = cache.join(app_id);
    let hashed_icon = fs::read_dir(directory)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            matches!(
                path.extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| extension.to_ascii_lowercase()),
                Some(extension) if matches!(extension.as_str(), "jpg" | "jpeg" | "png")
            ) && path
                .file_stem()
                .and_then(|name| name.to_str())
                .is_some_and(is_icon_hash)
        })
        .min_by_key(|path| {
            path.metadata()
                .map(|metadata| metadata.len())
                .unwrap_or(u64::MAX)
        })?;
    read_icon(&hashed_icon)
}

pub(crate) fn executable_path(app_id: &str, process_name: &str) -> Option<PathBuf> {
    if app_id.is_empty()
        || !app_id.chars().all(|character| character.is_ascii_digit())
        || process_name.is_empty()
    {
        return None;
    }

    let install_dir = list_installed_games()
        .ok()?
        .into_iter()
        .find(|game| game.app_id == app_id)?
        .install_dir;
    find_executable(Path::new(&install_dir), process_name)
}

fn find_executable(root: &Path, process_name: &str) -> Option<PathBuf> {
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                directories.push(entry.path());
            } else if file_type.is_file()
                && entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(process_name)
            {
                return Some(entry.path());
            }
        }
    }
    None
}

fn is_icon_hash(stem: &str) -> bool {
    stem.len() == 40 && stem.chars().all(|character| character.is_ascii_hexdigit())
}

fn read_icon(path: &Path) -> Option<(String, Vec<u8>)> {
    let mime_type = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        _ => return None,
    };
    let metadata = path.metadata().ok()?;
    if metadata.len() == 0 || metadata.len() > 256 * 1024 {
        return None;
    }
    Some((mime_type.to_string(), fs::read(path).ok()?))
}

fn locate_steam() -> Option<PathBuf> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey("Software\\Valve\\Steam") {
        for value_name in ["SteamPath", "InstallPath"] {
            if let Ok(value) = key.get_value::<String, _>(value_name) {
                let path = PathBuf::from(value.replace('/', "\\"));
                if path.is_dir() {
                    return Some(path);
                }
            }
        }
    }

    std::env::var_os("PROGRAMFILES(X86)")
        .map(PathBuf::from)
        .map(|path| path.join("Steam"))
        .filter(|path| path.is_dir())
}

fn parse_manifest(path: &Path, steamapps: &Path) -> Option<SteamGame> {
    let content = fs::read_to_string(path).ok()?;
    parse_manifest_content(&content, steamapps)
}

fn parse_manifest_content(content: &str, steamapps: &Path) -> Option<SteamGame> {
    let parsed = keyvalues_parser::parse(content.trim_start_matches('\u{feff}')).ok()?;
    let object = parsed.value.get_obj()?;
    let app_id = object_string(object, "appid")?;
    let name = object_string(object, "name")?;
    let install_folder = object_string(object, "installdir")?;

    if !app_id.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }

    Some(SteamGame {
        app_id: app_id.to_string(),
        name: name.to_string(),
        install_dir: steamapps
            .join("common")
            .join(install_folder)
            .to_string_lossy()
            .into_owned(),
    })
}

fn parse_library_paths(content: &str) -> Vec<PathBuf> {
    let Ok(parsed) = keyvalues_parser::parse(content.trim_start_matches('\u{feff}')) else {
        return Vec::new();
    };
    let Some(object) = parsed.value.get_obj() else {
        return Vec::new();
    };

    object
        .values()
        .filter_map(|values| values.first().and_then(Value::get_obj))
        .filter_map(|library| object_string(library, "path"))
        .map(PathBuf::from)
        .collect()
}

fn object_string<'a>(object: &'a Obj<'a>, key: &str) -> Option<&'a str> {
    object.get(key)?.first()?.get_str()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{find_executable, is_icon_hash, parse_library_paths, parse_manifest_content};

    #[test]
    fn finds_a_process_executable_below_an_install_directory() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let executable = find_executable(&source, "WINAPI.RS").expect("file should be found");
        assert!(executable.ends_with(Path::new("display").join("winapi.rs")));
    }

    #[test]
    fn recognizes_steam_icon_cache_hashes() {
        assert!(is_icon_hash("b6018068070ab0e23561694c11f7950dd6f4c752"));
        assert!(!is_icon_hash("library_600x900"));
    }

    #[test]
    fn parses_steam_library_folders() {
        let content = r#"
            "libraryfolders"
            {
                "0" { "path" "C:\\Program Files (x86)\\Steam" }
                "1" { "path" "E:\\SteamLibrary" }
            }
        "#;
        assert_eq!(
            parse_library_paths(content),
            vec![
                PathBuf::from(r"C:\Program Files (x86)\Steam"),
                PathBuf::from(r"E:\SteamLibrary"),
            ]
        );
    }

    #[test]
    fn parses_steam_app_manifest() {
        let content = r#"
            "AppState"
            {
                "appid" "730"
                "name" "Counter-Strike 2"
                "installdir" "Counter-Strike Global Offensive"
            }
        "#;
        let game = parse_manifest_content(content, Path::new(r"E:\SteamLibrary\steamapps"))
            .expect("manifest should parse");

        assert_eq!(game.app_id, "730");
        assert_eq!(game.name, "Counter-Strike 2");
        assert_eq!(
            game.install_dir,
            r"E:\SteamLibrary\steamapps\common\Counter-Strike Global Offensive"
        );
    }
}
