import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Bell, MonitorOff, X } from "lucide-react";
import { SettingRow } from "./SettingRow";
import { HotkeyRecorder } from "./SearchBarTab";
import { useStoredString } from "./useStoredSetting";

/**
 * Settings > General > Shortcuts: Bloom's own keyboard shortcuts. They work
 * everywhere, also in full-screen apps and games, and whether or not the
 * search bar is on.
 */

const SHORTCUTS = [
	{
		key: "bloom-hotkey-screen-off",
		icon: MonitorOff,
		label: "Screen Off",
		desc: "Turns the screen off; music and videos keep playing. Any key or mouse move wakes it"
	},
	{
		key: "bloom-hotkey-notification-center",
		icon: Bell,
		label: "Notification Center",
		desc: "Opens the notification center (and closes it again)"
	}
] as const;

function ShortcutRow({
	settingKey,
	icon,
	label,
	desc,
	divider
}: {
	settingKey: string;
	icon: typeof Bell;
	label: string;
	desc: string;
	divider: boolean;
}) {
	const [combo, setCombo] = useStoredString(settingKey, "");
	const [ok, setOk] = useState<boolean | null>(null);
	useEffect(() => {
		const check = () =>
			void invoke<boolean | null>("app_hotkey_status", { key: settingKey })
				.then(setOk)
				.catch(() => {});
		check();
		const unlisten = listen("app-hotkeys-changed", check);
		return () => {
			unlisten.then((f) => f());
		};
	}, [settingKey]);
	return (
		<SettingRow
			icon={icon}
			label={label}
			desc={
				combo && ok === false
					? "Another app already uses this shortcut. Pick another (or close that app)"
					: desc
			}
			divider={divider}
		>
			<div className="sb-inline">
				<HotkeyRecorder value={combo} onChange={setCombo} />
				{combo && (
					<button
						type="button"
						className="extra-folder-remove"
						title="Remove the shortcut"
						onClick={() => setCombo("")}
					>
						<X size={13} />
					</button>
				)}
			</div>
		</SettingRow>
	);
}

export function ShortcutsSettings() {
	return (
		<>
			<div className="setting-group-label">Shortcuts</div>
			<div className="setting-group">
				{SHORTCUTS.map((s, i) => (
					<ShortcutRow
						key={s.key}
						settingKey={s.key}
						icon={s.icon}
						label={s.label}
						desc={s.desc}
						divider={i < SHORTCUTS.length - 1}
					/>
				))}
			</div>
			<div className="sb-note">
				Click a shortcut, then press the keys. They work everywhere, also in games, and stay
				when the search bar is off. A shortcut another app already has can't be used until
				that app lets it go.
			</div>
		</>
	);
}
