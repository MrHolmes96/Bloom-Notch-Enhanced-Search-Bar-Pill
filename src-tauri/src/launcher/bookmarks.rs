//! Bookmarks from Chromium-based browsers (Chrome, Edge, Brave, Vivaldi,
//! Opera, Chromium), read from each profile's "Bookmarks" file. The files are
//! only re-read when one of them changed.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

#[derive(Serialize, Clone)]
pub struct Bookmark {
    pub name: String,
    pub url: String,
    pub browser: String,
    /// Folder path inside the browser, e.g. "Bookmarks bar/Work".
    pub folder: String,
}

struct Cache {
    stamp: Vec<(PathBuf, Option<SystemTime>)>,
    bookmarks: Vec<Bookmark>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// (browser id, display name, user data folder)
fn browser_roots() -> Vec<(&'static str, &'static str, PathBuf)> {
    let local = std::env::var("LOCALAPPDATA").map(PathBuf::from).ok();
    let roaming = std::env::var("APPDATA").map(PathBuf::from).ok();
    let mut roots = Vec::new();
    if let Some(local) = local {
        for (id, name, rel) in [
            ("chrome", "Chrome", r"Google\Chrome\User Data"),
            ("edge", "Edge", r"Microsoft\Edge\User Data"),
            ("brave", "Brave", r"BraveSoftware\Brave-Browser\User Data"),
            ("vivaldi", "Vivaldi", r"Vivaldi\User Data"),
            ("chromium", "Chromium", r"Chromium\User Data"),
        ] {
            roots.push((id, name, local.join(rel)));
        }
    }
    if let Some(roaming) = roaming {
        roots.push((
            "opera",
            "Opera",
            roaming.join(r"Opera Software\Opera Stable"),
        ));
        roots.push((
            "opera",
            "Opera GX",
            roaming.join(r"Opera Software\Opera GX Stable"),
        ));
    }
    roots
}

/// Every Bookmarks file of the allowed browsers, with its browser's name.
fn bookmark_files(browsers: &[String]) -> Vec<(String, PathBuf)> {
    let mut files = Vec::new();
    for (id, name, root) in browser_roots() {
        if !browsers.is_empty() && !browsers.iter().any(|b| b == id) {
            continue;
        }
        // Opera keeps the file at the top of its folder.
        let direct = root.join("Bookmarks");
        if direct.is_file() {
            files.push((name.to_string(), direct));
        }
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let dir = entry.file_name().to_string_lossy().to_string();
            if dir == "Default" || dir.starts_with("Profile ") {
                let file = entry.path().join("Bookmarks");
                if file.is_file() {
                    files.push((name.to_string(), file));
                }
            }
        }
    }
    files
}

fn collect(node: &serde_json::Value, browser: &str, folder: &str, out: &mut Vec<Bookmark>) {
    match node.get("type").and_then(|t| t.as_str()) {
        Some("url") => {
            let url = node.get("url").and_then(|u| u.as_str()).unwrap_or("");
            if url.starts_with("http") {
                out.push(Bookmark {
                    name: node
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or(url)
                        .to_string(),
                    url: url.to_string(),
                    browser: browser.to_string(),
                    folder: folder.to_string(),
                });
            }
        }
        Some("folder") => {
            let name = node.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let path = if folder.is_empty() {
                name.to_string()
            } else {
                format!("{folder}/{name}")
            };
            if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
                for child in children {
                    collect(child, browser, &path, out);
                }
            }
        }
        _ => {}
    }
}

fn read_file(browser: &str, path: &Path, out: &mut Vec<Bookmark>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    if let Some(roots) = json.get("roots").and_then(|r| r.as_object()) {
        for root in roots.values() {
            if root.is_object() {
                collect(root, browser, "", out);
            }
        }
    }
}

/// All bookmarks of the given browsers ("chrome", "edge", "brave",
/// "vivaldi", "chromium", "opera"; empty for all), without duplicates.
#[tauri::command]
pub async fn launcher_bookmarks(browsers: Vec<String>) -> Vec<Bookmark> {
    tauri::async_runtime::spawn_blocking(move || {
        let files = bookmark_files(&browsers);
        let stamp: Vec<(PathBuf, Option<SystemTime>)> = files
            .iter()
            .map(|(_, p)| {
                (
                    p.clone(),
                    std::fs::metadata(p).and_then(|m| m.modified()).ok(),
                )
            })
            .collect();
        if let Ok(cache) = CACHE.lock() {
            if let Some(cache) = cache.as_ref() {
                if cache.stamp == stamp {
                    return cache.bookmarks.clone();
                }
            }
        }
        let mut bookmarks = Vec::new();
        for (browser, path) in &files {
            read_file(browser, path, &mut bookmarks);
        }
        let mut seen = std::collections::HashSet::new();
        bookmarks.retain(|b| seen.insert(b.url.clone()));
        if let Ok(mut cache) = CACHE.lock() {
            *cache = Some(Cache {
                stamp,
                bookmarks: bookmarks.clone(),
            });
        }
        bookmarks
    })
    .await
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_folders() {
        let json = serde_json::json!({
            "type": "folder", "name": "Bookmarks bar", "children": [
                {"type": "url", "name": "Bloom", "url": "https://github.com/x/bloom"},
                {"type": "folder", "name": "Work", "children": [
                    {"type": "url", "name": "Mail", "url": "https://mail.example.com"},
                    {"type": "url", "name": "Local", "url": "chrome://settings"}
                ]}
            ]
        });
        let mut out = Vec::new();
        collect(&json, "Brave", "", &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].folder, "Bookmarks bar/Work");
        assert_eq!(out[0].browser, "Brave");
    }
}
