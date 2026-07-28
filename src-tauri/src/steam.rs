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

    use super::{parse_library_paths, parse_manifest_content};

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
