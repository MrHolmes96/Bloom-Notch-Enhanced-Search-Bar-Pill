//! What Bloom's window memory (`window_memory.rs`) keeps and decides, without
//! any Windows calls, so it can be tested on its own.
//!
//! Per app (its program path): how Bloom treats it (`Mode`) and its windows.
//! A window with a fixed name for a title ("Steam", "Friends List") has an
//! entry of its own; windows titled after what they show ("notes.txt -
//! Notepad") share one entry per window class (name "").

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct Place {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub maximized: bool,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Every window of the app.
    #[default]
    All,
    /// Only the windows ticked in Settings.
    Main,
    /// Bloom leaves the app alone.
    Off,
}

impl Mode {
    pub fn parse(text: &str) -> Option<Mode> {
        match text {
            "all" => Some(Mode::All),
            "main" => Some(Mode::Main),
            "off" => Some(Mode::Off),
            _ => None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct WinMem {
    pub class: String,
    /// The window's fixed name; "" for the entry shared by its app's windows
    /// whose titles change.
    #[serde(default)]
    pub name: String,
    /// The last title seen (what Settings shows for a shared entry).
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub place: Option<Place>,
    /// Ticked in Settings (counts when the app is on "Main window only").
    #[serde(default)]
    pub main: bool,
    /// When it was last saved (seconds since 1970), to keep the newest.
    #[serde(default)]
    pub used: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct AppMem {
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub windows: BTreeMap<String, WinMem>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct Store {
    pub version: u32,
    #[serde(default)]
    pub apps: BTreeMap<String, AppMem>,
}

pub const VERSION: u32 = 2;
/// Windows kept per app (chat windows and the like would add up forever).
pub const MAX_WINDOWS: usize = 16;

/// A title that names what the window shows rather than the window itself:
/// "notes.txt - Notepad", "Inbox | Outlook", or none at all.
pub fn is_document_title(title: &str) -> bool {
    let t = title.trim();
    t.is_empty()
        || [" - ", " – ", " — ", " | ", " · "]
            .iter()
            .any(|s| t.contains(s))
}

/// The window's name for its entry ("" = its app's shared entry).
pub fn window_name(title: &str) -> String {
    if is_document_title(title) {
        String::new()
    } else {
        title.trim().to_string()
    }
}

pub fn window_id(class: &str, name: &str) -> String {
    format!("{class}|{name}")
}

#[derive(Debug, PartialEq)]
pub enum Choice {
    /// Put it here (the id of the entry used).
    Use(String, Place),
    /// The app is Off in Settings.
    Off,
    /// The app is on "Main window only" and this window isn't ticked.
    NotMain,
    /// Nothing saved for it: the app places it.
    Nothing,
}

/// Where a window that just opened goes.
pub fn choose(app: Option<&AppMem>, class: &str, name: &str) -> Choice {
    let Some(app) = app else {
        return Choice::Nothing;
    };
    if app.mode == Mode::Off {
        return Choice::Off;
    }
    let own_id = window_id(class, name);
    let own = app.windows.get(&own_id);
    if app.mode == Mode::Main && !own.is_some_and(|w| w.main) {
        return Choice::NotMain;
    }
    if let Some(place) = own.and_then(|w| w.place) {
        return Choice::Use(own_id, place);
    }
    // A named window with no entry of its own yet takes the shared one only
    // while the app has no other named windows (an app with one window, or
    // sizes saved before windows were told apart). Otherwise it would take
    // another window's size, as Steam's Friends List took Steam's.
    if !name.is_empty() {
        let others = app
            .windows
            .iter()
            .any(|(id, w)| w.class == class && !w.name.is_empty() && *id != own_id);
        let shared_id = window_id(class, "");
        if let (false, Some(place)) = (others, app.windows.get(&shared_id).and_then(|w| w.place)) {
            return Choice::Use(shared_id, place);
        }
    }
    Choice::Nothing
}

/// The user moved or resized a window: keep it under the name it had when it
/// opened (a window that retitles itself, as Steam's chat does per tab, stays
/// one entry). Whether anything changed.
pub fn learn(
    app: &mut AppMem,
    class: &str,
    name: &str,
    title: &str,
    place: Place,
    now: u64,
) -> bool {
    if app.mode == Mode::Off {
        return false;
    }
    let id = window_id(class, name);
    let entry = app.windows.entry(id.clone()).or_insert_with(|| WinMem {
        class: class.to_string(),
        name: name.to_string(),
        ..Default::default()
    });
    let changed = entry.place != Some(place) || entry.title != title.trim();
    entry.place = Some(place);
    entry.title = title.trim().to_string();
    entry.used = now;
    if app.windows.len() > MAX_WINDOWS {
        // The oldest that isn't ticked, a shared entry or this one.
        let oldest = app
            .windows
            .iter()
            .filter(|(k, w)| **k != id && !w.main && !w.name.is_empty())
            .min_by_key(|(_, w)| w.used)
            .map(|(k, _)| k.clone());
        if let Some(oldest) = oldest {
            app.windows.remove(&oldest);
        }
    }
    changed
}

/// "Main window only" was picked with nothing ticked: tick the window named
/// like the app, else its most used named window, else the shared one.
pub fn tick_default_main(app: &mut AppMem, app_name: &str) {
    if app.windows.values().any(|w| w.main) {
        return;
    }
    let wanted = app_name.trim().to_lowercase();
    let pick = app
        .windows
        .iter()
        .find(|(_, w)| !w.name.is_empty() && w.name.to_lowercase() == wanted)
        .or_else(|| {
            app.windows
                .iter()
                .filter(|(_, w)| !w.name.is_empty())
                .max_by_key(|(_, w)| w.used)
        })
        .or_else(|| app.windows.iter().next())
        .map(|(k, _)| k.clone());
    if let Some(pick) = pick {
        if let Some(w) = app.windows.get_mut(&pick) {
            w.main = true;
        }
    }
}

/// Forget an app's sizes; ticks and its mode stay. Whether to drop the app.
pub fn forget_sizes(app: &mut AppMem) -> bool {
    app.windows.retain(|_, w| w.main);
    for w in app.windows.values_mut() {
        w.place = None;
    }
    app.mode == Mode::All && app.windows.is_empty()
}

/// Reads window_positions.json, including the first version's
/// `{"program path|class": place}`.
pub fn read_store(text: &str) -> Store {
    if let Ok(store) = serde_json::from_str::<Store>(text) {
        if store.version >= VERSION {
            return store;
        }
    }
    let mut store = Store {
        version: VERSION,
        ..Default::default()
    };
    if let Ok(old) = serde_json::from_str::<HashMap<String, Place>>(text) {
        for (key, place) in old {
            let Some((path, class)) = key.split_once('|') else {
                continue;
            };
            store
                .apps
                .entry(path.to_string())
                .or_default()
                .windows
                .insert(
                    window_id(class, ""),
                    WinMem {
                        class: class.to_string(),
                        place: Some(place),
                        ..Default::default()
                    },
                );
        }
    }
    store
}

#[cfg(test)]
mod tests {
    use super::*;

    fn learn(app: &mut AppMem, class: &str, title: &str, place: Place, now: u64) -> bool {
        super::learn(app, class, &window_name(title), title, place, now)
    }

    fn place(w: i32, h: i32) -> Place {
        Place {
            left: 100,
            top: 100,
            right: 100 + w,
            bottom: 100 + h,
            maximized: false,
        }
    }

    #[test]
    fn titles_are_told_apart() {
        assert_eq!(window_name("Steam"), "Steam");
        assert_eq!(window_name("Friends List"), "Friends List");
        assert_eq!(window_name("notes.txt - Notepad"), "");
        assert_eq!(window_name("Inbox | Outlook"), "");
        assert_eq!(window_name("   "), "");
    }

    #[test]
    fn steams_windows_keep_their_own_sizes() {
        let mut steam = AppMem::default();
        learn(&mut steam, "SDL_app", "Steam", place(1200, 800), 1);
        learn(&mut steam, "SDL_app", "Friends List", place(300, 700), 2);
        assert_eq!(
            choose(Some(&steam), "SDL_app", "Steam"),
            Choice::Use("SDL_app|Steam".into(), place(1200, 800))
        );
        assert_eq!(
            choose(Some(&steam), "SDL_app", "Friends List"),
            Choice::Use("SDL_app|Friends List".into(), place(300, 700))
        );
        // A window never resized is left to Steam, not given another's size.
        assert_eq!(choose(Some(&steam), "SDL_app", "Alice"), Choice::Nothing);
    }

    #[test]
    fn sizes_saved_before_this_version_still_apply() {
        let old = r#"{"c:\\apps\\lm studio.exe|Chrome_WidgetWin_1":{"left":1,"top":2,"right":901,"bottom":702,"maximized":false}}"#;
        let store = read_store(old);
        let app = store.apps.get("c:\\apps\\lm studio.exe").unwrap();
        let want = Place {
            left: 1,
            top: 2,
            right: 901,
            bottom: 702,
            maximized: false,
        };
        assert_eq!(
            choose(Some(app), "Chrome_WidgetWin_1", "LM Studio"),
            Choice::Use("Chrome_WidgetWin_1|".into(), want)
        );
        // And the new format reads back as written.
        let text = serde_json::to_string(&store).unwrap();
        assert_eq!(read_store(&text), store);
    }

    #[test]
    fn document_windows_share_one_size() {
        let mut notepad = AppMem::default();
        learn(
            &mut notepad,
            "Notepad",
            "a.txt - Notepad",
            place(800, 600),
            1,
        );
        assert_eq!(
            choose(Some(&notepad), "Notepad", ""),
            Choice::Use("Notepad|".into(), place(800, 600))
        );
    }

    #[test]
    fn off_and_main_only() {
        let mut steam = AppMem::default();
        learn(&mut steam, "SDL_app", "Steam", place(1200, 800), 1);
        learn(&mut steam, "SDL_app", "Friends List", place(300, 700), 2);
        steam.mode = Mode::Main;
        tick_default_main(&mut steam, "Steam");
        assert!(steam.windows["SDL_app|Steam"].main);
        assert!(!steam.windows["SDL_app|Friends List"].main);
        assert!(matches!(
            choose(Some(&steam), "SDL_app", "Steam"),
            Choice::Use(..)
        ));
        assert_eq!(
            choose(Some(&steam), "SDL_app", "Friends List"),
            Choice::NotMain
        );
        steam.mode = Mode::Off;
        assert_eq!(choose(Some(&steam), "SDL_app", "Steam"), Choice::Off);
        assert!(!learn(&mut steam, "SDL_app", "Steam", place(10, 10), 3));
        assert_eq!(steam.windows["SDL_app|Steam"].place, Some(place(1200, 800)));
    }

    #[test]
    fn the_oldest_named_window_goes_first() {
        let mut app = AppMem::default();
        learn(&mut app, "C", "x - doc", place(500, 500), 0);
        for i in 0..MAX_WINDOWS as u64 + 3 {
            learn(&mut app, "C", &format!("Chat {i}"), place(400, 400), i + 1);
        }
        assert_eq!(app.windows.len(), MAX_WINDOWS);
        assert!(app.windows.contains_key("C|"));
        assert!(!app.windows.contains_key("C|Chat 0"));
        assert!(app
            .windows
            .contains_key(&format!("C|Chat {}", MAX_WINDOWS + 2)));
    }

    #[test]
    fn forgetting_keeps_ticks_and_modes() {
        let mut app = AppMem::default();
        learn(&mut app, "C", "Main", place(500, 500), 1);
        learn(&mut app, "C", "Other", place(300, 300), 2);
        app.windows.get_mut("C|Main").unwrap().main = true;
        app.mode = Mode::Main;
        assert!(!forget_sizes(&mut app));
        assert_eq!(app.windows.len(), 1);
        assert_eq!(app.windows["C|Main"].place, None);
        let mut plain = AppMem::default();
        learn(&mut plain, "C", "Main", place(500, 500), 1);
        assert!(forget_sizes(&mut plain));
    }
}
