use std::path::PathBuf;

pub fn find_cursor(theme_name: &str, cursor_name: &str) -> Option<PathBuf> {
    let mut search_paths = vec![PathBuf::from(format!(
        "/usr/share/icons/{}/cursors",
        theme_name
    ))];

    if let Some(mut data_dir) = dirs::data_dir() {
        data_dir.push("icons");
        data_dir.push(theme_name);
        data_dir.push("cursors");
        search_paths.push(data_dir);
    }

    if let Some(mut home_dir) = dirs::home_dir() {
        home_dir.push(".icons");
        home_dir.push(theme_name);
        home_dir.push("cursors");
        search_paths.push(home_dir);
    }

    for base in search_paths {
        let cursor_path = base.join(cursor_name);
        if cursor_path.exists() && cursor_path.is_file() {
            return Some(cursor_path);
        }
    }

    None
}
