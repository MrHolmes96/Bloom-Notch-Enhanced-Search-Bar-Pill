# Bloom Settings Reference

All settings are stored in `settings.json` in Bloom's data folder (`%APPDATA%\com.mrholmes.bloom\`, opened from Settings > About > Data). The file is a flat JSON object with `bloom-` prefixed keys. Bloom watches this file for external changes and applies them in real-time.

## Quick Start

Edit `settings.json` with any text editor while Bloom is running. Changes are applied immediately — no restart required.

```json
{
	"bloom-dock-enabled": "true",
	"bloom-dock-mode": "smart",
	"bloom-theme-mode": "dark",
	"bloom-scale": "1.0"
}
```

## Settings Keys

### Dock

| Key                                | Type                             | Default       | Description                                                                                                                                                                      |
| ---------------------------------- | -------------------------------- | ------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bloom-dock-enabled`               | `"true"` / `"false"`             | `"true"`      | Show or hide the Bloom Dock (taskbar replacement).                                                                                                                               |
| `bloom-dock-mode`                  | `"fixed"` / `"smart"` / `"peek"` | `"smart"`     | Dock visibility behavior. **fixed** = always visible as AppBar. **smart** = auto-hide when overlapped by fullscreen apps. **peek** = hidden until cursor approaches bottom edge. |
| `bloom-dock-edge-delay`            | milliseconds `"0"`–`"2000"`      | `"200"`       | In smart/peek mode, how long the cursor must rest against the bottom screen edge before the dock appears. `"0"` reveals instantly.                                               |
| `bloom-dock-preview-enabled`       | `"true"` / `"false"`             | `"true"`      | Show window thumbnail previews when hovering dock icons.                                                                                                                         |
| `bloom-dock-icon-only`             | `"true"` / `"false"`             | `"false"`     | Minimal icon-only style (no background/padding around icons).                                                                                                                    |
| `bloom-dock-separator-enabled`     | `"true"` / `"false"`             | `"true"`      | Show a divider between pinned and unpinned dock apps when unpinned apps are present. Unpinned apps remain on the right when disabled.                                            |
| `bloom-dock-divider-opacity`      | `"0"`–`"100"`                    | `"24"`        | How visible the dock's dividers are, in percent: the one before unpinned apps, the ones around Dock Extras and the one before the bin.                                         |
| `bloom-start-icon`                 | icon key / `custom:<data URI>`   | `"default"`   | Start button icon: `default`, `bloom-colorful`, `bloom-golden`, `bloom-biscuit`, `windows`, or `custom:` followed by an uploaded image's data URI.                               |
| `bloom-dock-adaptive`              | `"true"` / `"false"`             | `"false"`     | Fixed dock only. Stretch the dock to full width like a traditional taskbar while a window is maximized, and contract back when it's restored.                                    |
| `bloom-dock-win-number-enabled`    | `"true"` / `"false"`             | `"true"`      | When the taskbar is replaced, Win+1 through Win+9 activate the matching pinned dock app (focus/minimize if running, launch otherwise) instead of the native taskbar slots.       |
| `bloom-dock-extras-position`       | `"off"` / `"left"` / `"right"`   | `"off"`       | Show a dock extras section with drives and folders beside the app icons, on the left or right side of the dock. |
| `bloom-dock-extras-drives`         | `"true"` / `"false"`             | `"true"`      | Show fixed, removable (with media) and connected mapped network drives in the dock extras. Empty card readers and unloaded discs are hidden.                                     |
| `bloom-dock-extras-recycle-bin`    | `"true"` / `"false"`             | `"true"`      | Show the Recycle Bin at the end of the dock (on its own, whether or not Dock Extras is on). Its icon shows how full the bin is. |
| `bloom-dock-bin-position` | `"end"` / `"start"` | `"end"` | Where the Recycle Bin sits: its own section at the right end of the dock, or right after Start. |
| `bloom-dock-bin-icons` | `"traybin"` / `"windows"` | `"traybin"` | Recycle Bin icons: Tray Bin's bin, drawn sharp at the dock's size, with one bar for anything under 100 MB and one more at 100 MB, 500 MB, 1 GB and 5 GB (five, full); or the Windows ones. The `"bloom"` value of an earlier build means `"traybin"`. |
| `bloom-dock-bin-only-empty-full` | `"true"` / `"false"` | `"false"` | Use only the empty and full icons instead of the 100 MB / 500 MB / 1 GB / 5 GB steps. |
| `bloom-dock-bin-levels` | `"5"` | absent | Set once when "Only Empty and Full Icons" was switched off so the five size bars apply (it could be left on by an earlier build). |
| `bloom-dock-bin-double-click` | `"open"` / `"empty"` | `"open"` | What a double click on the Recycle Bin does. With `"empty"`, a single click still opens it. |
| `bloom-dock-bin-confirm` | `"true"` / `"false"` | `"true"` | Ask before emptying the Recycle Bin. |
| `bloom-dock-bin-sound` | `"true"` / `"false"` | `"true"` | Play the Windows sound when emptying. |
| `bloom-dock-bin-progress` | `"true"` / `"false"` | `"true"` | Show the progress window while emptying. |
| `bloom-dock-extras-folders`        | comma-separated ids              | `"downloads"` | Known folders in the dock extras: any of `desktop`, `downloads`, `documents`, `pictures`, `music`, `videos`, or `"none"` for no folders.                                         |
| `bloom-dock-extras-custom-folders` | JSON array                       | `"[]"`        | User-added folders, e.g. `[{"name":"Projects","path":"D:\\Projects"}]`. Folders that no longer exist are skipped until they return.                                              |

### Notch

| Key                      | Type                             | Default   | Description                                                                                                                    |
| ------------------------ | -------------------------------- | --------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `bloom-notch-mode`       | `"fixed"` / `"smart"` / `"peek"` | `"fixed"` | Notch (top bar) visibility behavior. Same modes as dock. **peek** shows the notch briefly on media events and notifications.   |
| `bloom-notch-edge-delay` | milliseconds `"0"`–`"2000"`      | `"200"`   | In smart/peek mode, how long the cursor must rest against the top screen edge before the notch peeks. `"0"` reveals instantly. |
| `bloom-notch-widgets-on-hover` | `"true"` / `"false"`  | `"true"`  | Only show the status widgets (battery, weather, ...) while the notch is hovered. `"false"` shows them whenever the notch is visible, including when a hidden notch is revealed from the top edge. |
| `bloom-notch-shelf-in-scroll` | `"true"` / `"false"` | `"true"` | Scrolling or swiping on the notch also reaches the shelf, shown even when it is empty. |
| `bloom-notch-notes-in-scroll` | `"true"` / `"false"` | `"true"` | Scrolling or swiping on the notch also reaches the notes and to-dos panel. |
| `bloom-notch-notes` | JSON array of `{ id, text, todo, done, created }` | `"[]"` | The notch's quick notes and to-dos, newest first. |
| `bloom-control-center-buttons` | comma-separated ids, or `"none"` | `"dnd,energy-saver,tray,notifications,bloom-settings,restart-bloom"` | The control center's round buttons, in order. Also: `night-light`, `airplane`, `hotspot`, `rotation-lock`, `live-captions`, `project`, `cast`, `keyboard-layout`, `accessibility`, `nearby-sharing`, `vpn`, `color-filters`. |
| `bloom-tray-placement` | JSON object of icon key → `"shown"` / `"hidden"` | `"{}"` | Tray icons shown or hidden in the notch's tray when changed in Bloom (hidden ones sit in its "Hidden icons" menu). Keys are the program's file name (plus Windows' icon id when one program has several icons). Icons not listed are shown, unless Tray Icon Manager hides them. Bloom only; Windows' tray isn't changed. |
| `bloom-tray-live` | `"true"` / `"false"` | `"true"` | Bloom hosts the tray icons itself (passing everything on to Explorer), so the notch's tray shows each icon's live picture, updates instantly and clicks open at once. `"false"` reads icons from Windows' tray instead. Takes effect after restarting Bloom. |
| `bloom-tray-follow-manager` | `"true"` / `"false"` | `"true"` | Icons Tray Icon Manager hides (its `HiddenIcons` in `%AppData%\TrayIconManager\config.json`, read only, in any text encoding) stay hidden in the notch's tray. |

### Weather

| Key                              | Type                         | Default     | Description                                                                        |
| -------------------------------- | ---------------------------- | ----------- | ---------------------------------------------------------------------------------- |
| `bloom-weather-enabled`          | `"true"` / `"false"`         | `"true"`    | Show weather widget in the notch status bar.                                       |
| `bloom-weather-city`             | string                       | `""`        | City picked for the weather. Empty = no weather (Bloom never looks up where the PC is). |
| `bloom-weather-lat`              | number string                | (auto)      | Latitude coordinate for weather. Set automatically when a city is selected.        |
| `bloom-weather-lon`              | number string                | (auto)      | Longitude coordinate for weather. Set automatically when a city is selected.       |
| `bloom-weather-cached-temp`      | number string                | (none)      | Cached temperature value shown before next API fetch.                              |
| `bloom-weather-cached-condition` | string                       | (none)      | Cached weather condition text (e.g. "Partly Cloudy").                              |
| `bloom-temp-unit`                | `"celsius"` / `"fahrenheit"` | `"celsius"` | Temperature display unit.                                                          |

### Modules

| Key                         | Type                              | Default  | Description                                                                                              |
| --------------------------- | --------------------------------- | -------- | -------------------------------------------------------------------------------------------------------- |
| `bloom-calendar-enabled`    | `"true"` / `"false"`              | `"true"` | Enable calendar/timer mode in the notch.                                                                 |
| `bloom-music-mode-enabled`  | `"true"` / `"false"`              | `"true"` | Enable interactive music media widget.                                                                   |
| `bloom-music-compact-notch` | `"true"` / `"false"`              | `"true"` | Show compact music display (visualizer + artwork) in collapsed notch.                                    |
| `bloom-music-rest` | `"player"` / `"widgets"` / `"compact"` | `"player"` | What the notch shows at rest while music plays (Settings > Notch > While Music Plays): the full player (the same as when pointed at), the clock and widgets with the full player under them, or the small pill whose artwork opens the player. |
| `bloom-bt-bubble` | `"true"` / `"false"` | `"true"` | The battery bubble beside the notch while a Bluetooth device is connected: each device's type and battery (Settings > Notch > Bluetooth). A click opens the Bluetooth page. |
| `bloom-bt-low-battery` | number string (percent), `"0"` = off | `"20"` | A notification when a connected Bluetooth device is at this level or less, once until it is charged above it. Batteries are red at this level. |
| `bloom-notch-notes-view` | `"all"` / `"notes"` / `"todos"` | `"all"` | What the notch's Notes page lists: both sections (Notes and To-Do), or one of them. |
| `bloom-notch-notes-sort` | `"newest"` / `"oldest"` / `"az"` / `"za"` | `"newest"` | The order of the Notes page's lists (open to-dos always come before done ones). |
| `bloom-notch-remember-panel` | `"true"` / `"false"` | `"true"` | Click opens the last-used panel when on; when off, click opens `bloom-notch-default-panel`. |
| `bloom-notch-default-panel` | `"calendar"` / `"command-center"` / `"shelf"` / `"notes"` / `"blitzit"` | `"calendar"` | Page opened on click when panel memory is off (Settings > Notch > Pages > Opens On). If that page is off, Controls opens instead. |
| `bloom-notch-page-order` | JSON array of page ids | `["calendar","command-center","shelf","notes","blitzit"]` | The order scrolling, swiping and the arrows go through the notch's pages (Settings > Notch > Pages). Pages missing from it go at the end; each page is switched on or off by its own setting. |
| `bloom-notch-last-panel`    | `"calendar"` / `"command-center"` | (unset)  | Last panel the notch was left on, saved by Bloom while panel memory is on. |
| `bloom-notch-click-to-open` | `"true"` / `"false"` | (legacy) | Old setting read only when `bloom-notch-remember-panel` is absent; `"false"` now selects fixed-panel behavior. |

### Shelf

| Key                   | Type                 | Default   | Description                                                                                                    |
| --------------------- | -------------------- | --------- | -------------------------------------------------------------------------------------------------------------- |
| `bloom-shelf-persist` | `"true"` / `"false"` | `"false"` | Keep notch shelf items across restarts. Off = items are cleared when Bloom closes. Max 10 items, newest first. |
| `bloom-shelf-localsend` | `"true"` / `"false"` | `"true"` | A LocalSend button under the shelf's items: it turns picking on, and the picked items go to a nearby LocalSend device (found only while picking). |
| `bloom-shelf-localsend-receive-minutes` | number string | `"10"` | Receiving with LocalSend into the shelf (the shelf's "LocalSend: Enabled/Disabled" button) turns itself off after this many minutes without a transfer; `0` only when turned off. It is always off when Bloom starts. Received files are saved in Downloads. |

### Music Appearance

| Key                                | Type                      | Default     | Description                                                                                     |
| ---------------------------------- | ------------------------- | ----------- | ----------------------------------------------------------------------------------------------- |
| `bloom-media-layout`               | `"classic"` / `"compact"` | `"classic"` | Expanded player style. **classic** = large album art. **compact** = small thumbnail + controls. |
| `bloom-media-ambience-enabled`     | `"true"` / `"false"`      | `"true"`    | Colored ambient glow behind expanded album art.                                                 |
| `bloom-media-compact-glow-enabled` | `"true"` / `"false"`      | `"true"`    | Glow effect around the collapsed compact thumbnail.                                             |
| `bloom-media-visualizer-enabled`   | `"true"` / `"false"`      | `"true"`    | Audio visualizer bars in music mode. Also accepts `bloom-visualizer-enabled` (legacy alias).    |
| `bloom-media-album-art-enabled`    | `"true"` / `"false"`      | `"true"`    | Show album artwork in the notch music display.                                                  |

### Overlays

| Key                                | Type                 | Default  | Description                                                              |
| ---------------------------------- | -------------------- | -------- | ------------------------------------------------------------------------ |
| `bloom-volume-overlay-enabled`     | `"true"` / `"false"` | `"true"` | Show Bloom volume HUD when volume changes (replaces native Windows OSD). |
| `bloom-volume-edge-enabled`        | `"true"` / `"false"` | `"true"` | Trigger volume HUD by hovering the left screen edge.                     |
| `bloom-brightness-overlay-enabled` | `"true"` / `"false"` | `"true"` | Show Bloom brightness HUD when brightness changes.                       |
| `bloom-brightness-edge-enabled`    | `"true"` / `"false"` | `"true"` | Trigger brightness HUD by hovering the right screen edge.                |
| `bloom-brightness-key-step` | `"1"` / `"2"` / `"5"` / `"0"` | `"2"` | Percent a brightness key changes the brightness by, on any keyboard: keys that send HID brightness codes (seen through Raw Input) and laptop keys handled by the firmware. Bloom replaces Windows' 10% step; without a built-in screen the keys change external monitors over DDC/CI. `"0"` keeps Windows' own step. |

### Windows

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `bloom-window-memory` | `"true"` / `"false"` | `"true"` | Remember each app's window size and position (in `window_positions.json`) and restore it when the app opens again. |
| `bloom-windows-snap-borders` | `"true"` / `"false"` | `"true"` | Windows snapped by Windows 11 (halves, thirds, quarters, Snap Layouts) lose Windows 11's border and get Bloom's own (below) instead; once they are no longer snapped (and when Bloom quits) they follow Windows' setting: no border, or the accent border when "Show accent colour on title bars and window borders" is on. Settings > Appearance > Window Borders. |
| `bloom-windows-snap-border-color` | `"#RRGGBB"` | `"#808080"` | Colour of Bloom's own border on snapped windows: one screen pixel wide along each edge. |
| `bloom-windows-snap-border-opacity` | number `"0"`–`"100"` | `"0"` | Opacity of that border in percent; `0` draws no border at all. |
| `bloom-dock-preview-style` | `"static"` / `"live"` | `"static"` | Dock window previews: a snapshot, or Windows' live preview of the window. |
| `bloom-notification-center` | `"windows"` / `"bloom"` | `"bloom"` with Bloom cards, else `"windows"` | What the notification button opens: Windows' notification center or Bloom's (top right, as tall as its notifications, scrolling past about six, grouped by app; buttons and replies work as in Windows). |

### Notifications

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `bloom-notify-style` | `"windows"` / `"bloom"` | `"windows"` | Windows' own pop-ups, or Bloom cards in Bloom's colors and opacity with the notification's buttons and reply box (Windows' pop-up is then hidden). |
| `bloom-notify-position` | `"default"` / `"top-right"` / `"top-left"` / `"bottom-right"` / `"bottom-left"` | `"default"` | Corner notifications come from, in either style. |
| `bloom-notify-radius` | number (px) | `"16"` | Corner rounding of Bloom cards. |
| `bloom-notify-duration` | number (seconds) | `"6"` | How long a Bloom card stays. |

### Integrations

| Key | Values | Default | Description |
| --- | --- | --- | --- |
| `bloom-notch-blitzit` | `"true"` / `"false"` | `"true"` | Scrolling or swiping on the notch also reaches Blitzit's page (add tasks, today's tasks, Blitz now). |
| `bloom-blitzit-server` | URL | `"https://core.blitzit.app/v3/mcp"` | Blitzit's connection for other apps (its MCP server). `https://integration-prd.blitzit.app/mcp` for Blitzit before version 3. The sign-in itself is not a setting: it is stored encrypted for this Windows user in `integrations\blitzit.bin` in Bloom's config folder. |
| `bloom-blitzit-tools` | JSON `{ create, lists, tasks, complete }` of tool names | `"{}"` | Which of Blitzit's actions Bloom uses, when chosen by hand in Settings > Integrations > Blitzit. Missing ones are found automatically from each action's name, description and inputs. |
| `bloom-blitzit-list` | JSON `{ id, name }` | none | The Blitzit list new tasks go in, last chosen in the notch. |
| `bloom-blitzit-column` | text | `""` | The column new tasks go in (Today, This week, …) when Blitzit offers a choice; Today when not set. |

### Search Bar Pill

| Key                                | Type                                   | Default       | Description                                                                                                       |
| ---------------------------------- | -------------------------------------- | ------------- | ----------------------------------------------------------------------------------------------------------------- |
| `bloom-launcher-enabled`           | `"true"` / `"false"`                   | `"true"`      | Turn the search bar and its shortcuts on or off.                                                                  |
| `bloom-launcher-hotkey`            | string                                 | `"Alt+Space"` | Shortcut that opens the search bar, e.g. `"Alt+Space"`, `"Ctrl+Shift+K"`, `"Win+F2"`.                             |
| `bloom-launcher-ignore-fullscreen` | `"true"` / `"false"`                   | `"false"`     | Ignore the shortcut while a full-screen app or game is in front.                                                  |
| `bloom-hotkey-screen-off` | shortcut text (`"Ctrl+Alt+B"`) or `""` | `""` | Settings > General > Shortcuts > Screen Off: turns the screen off (media keeps playing). Works everywhere, also with the search bar off. |
| `bloom-hotkey-notification-center` | shortcut text or `""` | `""` | Settings > General > Shortcuts > Notification Center: opens (and closes) the notification center the bell opens. |
| `bloom-launcher-position`          | `"notch"` / `"upper"` / `"center"`     | `"notch"`     | Where the search bar opens: under the notch, a quarter down the screen, or near the middle.                       |
| `bloom-launcher-screen`            | `"cursor"` / `"focus"` / `"primary"`   | `"cursor"`    | Which monitor it opens on: the one with the mouse, the one with the active window, or the main monitor.           |
| `bloom-launcher-query-hotkeys`     | JSON array of `{ "hotkey", "query" }`  | `"[]"`        | Extra shortcuts that open the search bar with a search already typed in.                                          |
| `bloom-launcher-config`            | JSON object                            | (built in)    | Everything else on the Search Bar Pill page: plugins, keywords, web searches, file search, shell and more.        |

### Appearance

| Key                      | Type                                             | Default     | Description                                                                                                                                                 |
| ------------------------ | ------------------------------------------------ | ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bloom-theme-mode`       | `"dark"` / `"light"` / `"custom"` / `"adaptive"` | `"dark"`    | Theme mode. **dark** = dark translucent. **light** = light translucent. **custom** = user-picked color. **adaptive** = follows Windows system accent color. |
| `bloom-theme-color`      | hex string                                       | `"#007aff"` | Custom theme color (used in `custom` and `adaptive` modes).                                                                                                 |
| `bloom-theme-opacity`    | float string                                     | `"0.80"`    | Background opacity (0.1 to 1.0).                                                                                                                            |
| `bloom-theme-saturation` | float string                                     | `"0.50"`    | Color saturation for custom/adaptive themes (0.0 to 1.0).                                                                                                   |
| `bloom-theme-brightness` | float string                                     | `"0.15"`    | Background brightness for custom/adaptive themes (0.0 to 1.0).                                                                                              |
| `bloom-corners-enabled`  | `"true"` / `"false"`                             | `"false"`   | Render rounded screen corner overlays on top edges.                                                                                                         |

### Status Widgets

| Key                    | Type        | Default                                    | Description                                                                                                 |
| ---------------------- | ----------- | ------------------------------------------ | ----------------------------------------------------------------------------------------------------------- |
| `bloom-status-widgets` | JSON string | `{"left":["weather"],"right":["battery"]}` | Widget layout in collapsed notch. Available: `"weather"`, `"battery"`, `"cpu"`, `"ram"`, `"disk"`, `"net"`. |

Example:

```json
{
	"bloom-status-widgets": "{\"left\":[\"cpu\",\"ram\"],\"right\":[\"battery\",\"net\"]}"
}
```

### System

| Key                            | Type                 | Default   | Description                                                                                          |
| ------------------------------ | -------------------- | --------- | ---------------------------------------------------------------------------------------------------- |
| `bloom-notch-scale` | number `"0.8"`–`"1.3"` | `bloom-scale`, else `"1.0"` | Size of the notch and its text, the volume and brightness pop-ups and the settings window (Settings > Appearance > Notch Size). |
| `bloom-dock-scale` | number `"0.8"`–`"1.3"` | `bloom-scale`, else `"1.0"` | Size of the dock, its icons and its menus (Settings > Appearance > Dock Size). |
| `bloom-scale`                  | float string         | `"1.0"`   | The one size for everything in older versions. A part without its own size (`bloom-notch-scale`, `bloom-dock-scale`) still uses it; Settings no longer changes it. |
| `bloom-low-battery-threshold`  | integer string       | `"20"`    | Battery percentage that triggers the low-battery alert pulse (5 to 50, step 5).                      |
| `bloom-time-format-24h`        | `"true"` / `"false"` | `"false"` | Use 24-hour clock format in the notch. When `"false"`, displays 12-hour format with AM/PM.           |

### Internal (Do Not Edit Manually)

| Key                 | Type     | Description                                                                           |
| ------------------- | -------- | ------------------------------------------------------------------------------------- |
| `bloom-first-run`   | sentinel | Set to `"done"` after first launch. Triggers splash screen if absent.                 |
| `bloom-app-version` | string   | Last known app version. If it differs from current, splash screen is shown on update. |

## Event System

Bloom uses two Tauri events for settings synchronization:

### `settings-changed`

- **Emitted by:** `save_setting` command (frontend or backend)
- **Payload:** `{ "key": "bloom-...", "value": ... }`
- **Purpose:** Broadcasts changes made through the Bloom UI to all windows
- **Key format:** Bloom-prefixed keys as-is (e.g. `"bloom-dock-mode"`)

### `settings-external-changed`

- **Emitted by:** File watcher (detects external edits to `settings.json`)
- **Payload:** `{ "key": "bloom-...", "value": ... }` or `{ "key": "bloom-...", "value": null }` for removed keys
- **Purpose:** Broadcasts changes made by external editors (VS Code, notepad, scripts)
- **Key format:** Bloom-prefixed keys as-is
- **Behavior:** Also syncs values to `localStorage` for instant frontend reads

### Flow

```
External editor saves settings.json
    ↓
File watcher detects change (ReadDirectoryChangesW)
    ↓
Diffs against SETTINGS_CACHE
    ↓
Emits settings-external-changed for each changed/removed key
    ↓
useSettingsSync hook updates React state + localStorage
    ↓
useSettingsSync hook updates React state
    ↓
UI re-renders with new values
```

## Example: Changing Dock Mode via Script

```powershell
# PowerShell: switch dock to smart mode
$json = Get-Content "$env:APPDATA\bloom\settings.json" | ConvertFrom-Json
$json.'bloom-dock-mode' = 'smart'
$json | ConvertTo-Json | Set-Content "$env:APPDATA\bloom\settings.json"
```

```python
# Python: disable dock
import json, os
path = os.path.join(os.environ['APPDATA'], 'bloom', 'settings.json')
with open(path) as f: settings = json.load(f)
settings['bloom-dock-enabled'] = 'false'
with open(path, 'w') as f: json.dump(settings, f)
```

## Notes

- All boolean values are strings (`"true"` / `"false"`) for consistency with `localStorage`.
- The `useSettingsSync` hook auto-converts `"true"` / `"false"` strings to booleans.
- `auto-hide` mode values in `bloom-dock-mode` and `bloom-notch-mode` are legacy aliases for `smart` — they are mapped automatically.
- Changing `bloom-notch-scale`, `bloom-dock-scale` or `bloom-scale` triggers AppBar re-registration to adjust reserved screen space.
- Settings → General → Reset to Defaults removes every `bloom-` key from `settings.json` and `localStorage` (except the `bloom-first-run` / `bloom-app-version` lifecycle sentinels), then restarts Bloom. Pinned apps (`pinned_apps.json`) and uploaded icons (`custom_icons/`) are preserved.
- Theme changes (`bloom-theme-*`) are applied by reading all theme values from `localStorage` and calling `applyTheme()` — the theme system depends on all five theme keys being in sync.
