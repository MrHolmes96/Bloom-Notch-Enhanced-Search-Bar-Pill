# Bloom: how it sits in Windows, and how to get Windows back

Bloom runs alongside Explorer. Explorer stays Windows' shell (Bloom never
changes Windows' "Shell" setting), so everything Explorer provides keeps
working: Start, Alt+Tab, Snap layouts, virtual desktops, Windows'
notifications, Store apps, jump lists and the tray. While Bloom runs it keeps
Explorer's taskbar hidden and in auto-hide, and its own dock and notch take
its place. Windows' taskbar comes back only when Bloom is quit or uninstalled;
it stays hidden across sign-out, shutdown, Bloom restarts and Bloom updates.

## Getting Windows back

Any of these puts back Windows' taskbar (with your own auto-hide setting and
position), Windows' volume, brightness and notification pop-ups and its task
list, and stops Bloom starting with Windows. Bloom stays installed.

- Press the Windows key, type **Bloom Recovery** and press Enter.
- Or press Ctrl+Shift+Esc (Task Manager) > Run new task, and run
  `"%LOCALAPPDATA%\Bloom\Bloom.exe" --recovery`.
- Or uninstall Bloom (Settings > Apps): it does the same before removing
  Bloom's files.

Start Bloom from the Start menu to use it again (it then starts with Windows
again when Settings > General > Startup > Launch at Login is on).

## Command line

| Command | What it does |
| --- | --- |
| `Bloom.exe --recovery` | Windows as it was, now (see above), with a message saying so |
| `Bloom.exe --disable` | The same, quietly |
| `Bloom.exe --enable` | Bloom starts with Windows again, and starts now |
| `Bloom.exe --repair` | Puts back what Bloom needs: its startup task pointing at this copy, and Explorer as Windows' shell (a left-over per-user Shell policy is removed) |
| `Bloom.exe --export-logs` | Bloom's logs and a health report in a zip on the desktop (no settings, sign-ins or notification texts) |
| `Bloom.exe --restore-windows` | What the uninstaller runs |

Bloom repairs itself once after Windows is updated to a new build (feature
updates can reset such things); what it did is noted in `crash.log` in
`%APPDATA%\com.mrholmes.bloom`.

## When something goes wrong

- Bloom crashed: its watchdog puts Windows' taskbar back at once and starts
  Bloom again (at most three times in ten minutes). The reason is in
  `crash.log`.
- A part of Bloom that apps talk to (the tray, the task list) stopped
  answering: Windows' task list goes straight back to Explorer, so no app
  waits on Bloom, and Bloom restarts.
- Safe Mode starts Windows without Bloom, as always.
