# Dock background apps and system actions

An unpinned app leaves the dock when its last window closes, even while it keeps
running in the tray, so tray-only apps never pile up on the dock. Pinned apps
show a grey dot from startup when they are already active in the tray. Windowed
apps retain their normal window dots and previews.
Running unpinned apps appear to the right of the pinned group. Settings > Dock >
App Separator controls the thin theme-aware divider; it is on by default and
appears only when an unpinned app is present.

- Click a background app to invoke its notification icon's default action.
- Right-click a background app to request its native tray context menu.
- Shift-right-click opens Bloom's menu for pinning and icon customization.
- An app with several tray icons has a separate menu entry for each icon.
- Start's context menu includes Task Manager, Disk Management, Device Manager,
  Computer Management and Windows settings shortcuts. The dock background menu
  includes Task Manager and Taskbar Settings.

## The tray in the notch

The control center's tray button opens the same icons inside the notch: every
live tray icon, wherever Windows keeps it (taskbar or overflow). Hidden icons
are one click away in its "Hidden icons" menu, where they still work. The
pencil switches to editing: hide an icon, or show a hidden one again. What is
changed is kept in `bloom-tray-placement` and only affects Bloom: `IsPromoted`
is read, never written, and no icon is changed in Windows. The view is never
shorter than the control center it opens from, so the pointer that clicked
the tray button stays inside the notch.

Settings > Notch > System Tray lists every live icon with a switch, and
follows Tray Icon Manager: the `HiddenIcons` in its
`%AppData%\TrayIconManager\config.json` (read only, in whatever text encoding
PowerShell wrote it, matched by icon GUID, then program, then name) are hidden
in Bloom's tray while "Follow Tray Icon Manager" is on. The settings say how
many it hides and name the ones not running, so a mismatch is visible. "Copy
Tray Icon Manager's Hidden Icons" copies them into Bloom's own list, so they
stay hidden without it.

## Implementation and compatibility

### Live tray (default)

`tray_host.rs` keeps a hidden, top-most window of the taskbar's class,
`Shell_TrayWnd`. `Shell_NotifyIcon` hands every icon change to the first window
of that class, so Bloom records each icon (window, id, GUID, callback message,
version, tooltip, hidden state, and a copy of its picture) and passes the
message on unchanged to Explorer's taskbar, which answers it. Explorer's tray
stays complete, so nothing changes for Windows or after Bloom quits. AppBar
messages, icon positions and commands sent to that class are passed straight
on as well. This is how RetroBar and Cairo show the tray alongside Explorer.

The AppBar messages that carry a screen area (`ABM_QUERYPOS`, `ABM_SETPOS`,
`ABM_GETTASKBARPOS`) keep it in shared memory made for the window they reach
first, which Explorer can't open when the message is passed on as it is. So
that memory is made again for Explorer, the message sent on with it, and
Explorer's answer copied back (as Cairo's `AppBarManager` does). Without this,
bars such as Bloom's fixed dock and notch register but never reserve their
strip, and maximized windows go underneath them.

At start Bloom broadcasts `TaskbarCreated` (what Explorer sends after a
restart), so running apps add their icons again and Bloom sees them all. Every
two seconds it checks that it is still the first window of that class (Explorer
raises its taskbar when it is shown); if not, it gets back in front and asks
for the icons again, at most every 30 seconds. After an Explorer restart it does
the same. Icons whose window is gone are dropped.

A click sends the app the callback messages Explorer sends for a click on its
icon (mouse move, button down and up, then `NIN_SELECT` or `WM_CONTEXTMENU` for
version 3 and 4 icons, with the pointer position for version 4), after giving
the app the right to come to the front. The app opens its menu or window at the
pointer at once. While a menu or flyout it opened is on screen, the notch stays
open, however long that is (`tray-interaction-end` tells it when they are
gone). Menus are Windows' own (`#32768`), the app's caption-less tool or
top-most pop-ups, and floating pop-ups of another process at the pointer (an
app drawing its menu in a helper process); another window the click opened
keeps the notch open for up to two minutes.

An icon's picture is loaded again at 64 px from the file Windows loaded it from
when that is known (`GetIconInfoEx`), so it stays sharp at the notch's size;
otherwise the app's own picture is used. Changes reach the notch as
`tray-changed`, at most every 120 ms.

Bloom's own code looks Explorer's taskbar up with `explorer_taskbar()`, which
skips Bloom's window of the same class. Settings > Notch > System Tray > Live
Tray (`bloom-tray-live`) turns this off; the registry-based reading below is
then used, and it is also the fallback before the first icons arrive.

### Without the live tray

`tray.rs` reads notification-icon candidates from Windows 11's
`HKCU\Control Panel\NotifyIconSettings`. Each candidate must match a running
process path and pass `Shell_NotifyIconGetRect`; historical entries alone never
count as running. Explorer-hosted system icons (including Bluetooth) are excluded
because their process path is shared with File Explorer windows. The frontend reconciles tray owners with actual window handles,
including applications launched through shell IDs. A shared browser tray does
not combine multiple PWAs. Pinned shell IDs also match visible app windows when
their executable paths differ, so opening Bloom's Settings window does not add
an unpinned duplicate of a pinned Bloom app.

Native interactions use UI Automation. Windows reports the chevron rectangle for
an icon hidden in the overflow, so hidden icons are matched by the live
`UIOrderList` order against Explorer's overflow buttons, with equal-count and
nonempty-label checks. Any label that uniquely identifies another icon must
also agree with that order. App-defined tooltip text may differ from the
executable description, so it cannot be required to contain the process name.
Promoted icons are matched to the
taskbar's XAML controls by screen position, accounting for display scaling.
Failure produces a user-visible error. Menu opening temporarily exposes the
native tray and restores the taskbar on completion or error, without changing
AppBar state or icon promotion preferences. Windows' connection/transaction
timeouts bound UIA provider calls.

The registry layout and Explorer's UIA tree are Windows implementation details.
Windows versions or alternative shells that do not expose them keep the existing
window-based dock behavior. Windows 10 tray discovery is not implemented here.
Background processes without notification icons are not included.

API references:

- [Shell_NotifyIconGetRect](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyicongetrect)
- [IUIAutomationElement3::ShowContextMenu](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationelement3-showcontextmenu)

## Validation

Automated checks:

```powershell
bun run build
bun test scripts/dockApps.test.ts
cd src-tauri
cargo check --locked
cargo clippy --locked --all-targets
cargo test --locked
# Optional read-only integration check against the current desktop:
cargo test --locked live_tray_discovery -- --ignored --nocapture
```

Manual acceptance checks (required before merging):

1. On Bloom startup, unrelated tray-only apps stay out of the dock; pinned apps
   already active in the tray show a grey dot.
2. Start an unpinned Discord with one window: one normal dot, no duplicate entry.
3. Close Discord to the tray: an unpinned Discord leaves the dock; a pinned one
   keeps one grey dot and no window thumbnail.
4. Right-click its dock icon: Discord's own menu appears and its actions work.
5. Click the background icon: Discord responds through its tray default action.
6. Quit Discord through its native menu: unpinned icon disappears; pinned dot clears
   within the 10-second safety poll.
7. Repeat with an icon in overflow, a promoted icon, and an app with multiple icons.
   Include apps whose tooltips differ from their executable names, such as
   Tailscale, Phone Link, and Windows Security.
8. Exit/restart Explorer, exit Bloom during menu opening, and toggle the dock off:
   the taskbar must return to the intended state with no transparent residue.
9. Check Start and dock system actions, including denied UAC and launch errors.
10. Check light/dark themes, large Bloom scale, multiple monitors and mixed DPI.
11. Toggle App Separator off and on: only the divider changes; unpinned apps stay
    on the right. Open pinned Bloom and check that Settings appears without a
    second unpinned Bloom icon. File Explorer must not show Bluetooth tray actions.

Live discovery, the Discord background dot, and Discord's native context menu
have been exercised on the development machine. The broader compatibility,
menu-action, and restoration checks above still require end-to-end validation.
