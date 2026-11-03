use std::path::{Path, PathBuf};

pub const ENV_VAR: &str = "TERRARIA_PATH";
pub const CONFIG_FILE: &str = "rusted-terraria.cfg";

/// Resolution order: `TERRARIA_PATH`, `terraria_path = ...` in `rusted-terraria.cfg`
/// (current dir then next to the executable), then the default Steam path.
pub fn terraria_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(ENV_VAR).filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    for dir in [Some(PathBuf::from(".")), exe_dir].into_iter().flatten() {
        if let Some(p) = read_config(&dir.join(CONFIG_FILE)) {
            return Some(p);
        }
    }
    default_dirs().into_iter().find(|p| p.is_dir())
}

/// `<install>/Content/Images`, where the Tiles_*.xnb / Wall_*.xnb live.
pub fn images_dir() -> Option<PathBuf> {
    terraria_dir().map(|p| p.join("Content").join("Images")).filter(|p| p.is_dir())
}

fn read_config(path: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "terraria_path").then(|| PathBuf::from(v.trim().trim_matches('"')))
    })
}

fn default_dirs() -> Vec<PathBuf> {
    let mut v = vec![PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Terraria")];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        v.push(home.join(".steam/steam/steamapps/common/Terraria"));
        v.push(home.join(".local/share/Steam/steamapps/common/Terraria"));
        v.push(home.join("Library/Application Support/Steam/steamapps/common/Terraria/Terraria.app/Contents/Resources"));
    }
    v
}
