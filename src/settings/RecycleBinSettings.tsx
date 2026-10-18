import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
	Bell,
	Image,
	LayoutGrid,
	MousePointerClick,
	MoveHorizontal,
	ShieldCheck,
	Volume2
} from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useSettingsSync } from "../hooks/useSettingsSync";

/** The dock bin's options, the same set Tray Bin has in its Configure menu. */

const KEYS = {
	position: "bloom-dock-bin-position",
	icons: "bloom-dock-bin-icons",
	onlyEmptyFull: "bloom-dock-bin-only-empty-full",
	doubleClick: "bloom-dock-bin-double-click",
	confirm: "bloom-dock-bin-confirm",
	sound: "bloom-dock-bin-sound",
	progress: "bloom-dock-bin-progress"
} as const;

function save(key: string, value: string) {
	localStorage.setItem(key, value);
	invoke("save_setting", { key, value }).catch(console.error);
}

function readFlag(key: string, fallback: boolean) {
	const value = localStorage.getItem(key);
	return value === null ? fallback : value !== "false";
}

function Toggle({ checked, onChange }: { checked: boolean; onChange: (next: boolean) => void }) {
	return (
		<label className="toggle-switch">
			<input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
			<span className="slider"></span>
		</label>
	);
}

export function RecycleBinSettings() {
	const [position, setPosition] = useState(() => localStorage.getItem(KEYS.position) ?? "end");
	const [icons, setIcons] = useState(() => {
		const stored = localStorage.getItem(KEYS.icons);
		return stored === "traybin" || stored === "windows" ? stored : "bloom";
	});
	const [onlyEmptyFull, setOnlyEmptyFull] = useState(() => readFlag(KEYS.onlyEmptyFull, false));
	const [doubleClick, setDoubleClick] = useState(
		() => localStorage.getItem(KEYS.doubleClick) ?? "open"
	);
	const [confirm, setConfirm] = useState(() => readFlag(KEYS.confirm, true));
	const [sound, setSound] = useState(() => readFlag(KEYS.sound, true));
	const [progress, setProgress] = useState(() => readFlag(KEYS.progress, true));
	const [trayBinFound, setTrayBinFound] = useState<boolean | null>(null);

	useSettingsSync({
		[KEYS.position]: (v) => setPosition(String(v)),
		[KEYS.icons]: (v) => setIcons(String(v) === "traybin" || String(v) === "windows" ? String(v) : "bloom"),
		[KEYS.onlyEmptyFull]: (v) => setOnlyEmptyFull(v !== false),
		[KEYS.doubleClick]: (v) => setDoubleClick(String(v)),
		[KEYS.confirm]: (v) => setConfirm(v !== false),
		[KEYS.sound]: (v) => setSound(v !== false),
		[KEYS.progress]: (v) => setProgress(v !== false)
	});

	useEffect(() => {
		if (icons !== "traybin") return;
		void invoke<boolean>("recycle_bin_icons_available")
			.then(setTrayBinFound)
			.catch(() => setTrayBinFound(false));
	}, [icons]);

	const flag = (key: string, set: (v: boolean) => void) => (next: boolean) => {
		set(next);
		save(key, String(next));
	};

	return (
		<>
			<SettingRow
				icon={MoveHorizontal}
				label="Bin Position"
				desc={
					position === "start"
						? "Right after Start, at the left end of the dock"
						: "In its own section at the right end of the dock"
				}
			>
				<select
					className="settings-select"
					value={position}
					onChange={(e) => {
						setPosition(e.target.value);
						save(KEYS.position, e.target.value);
					}}
				>
					<option value="end">End of Dock</option>
					<option value="start">Next to Start</option>
				</select>
			</SettingRow>
			<SettingRow
				icon={Image}
				label="Bin Icons"
				desc={
					icons === "traybin" && trayBinFound === false
						? "Tray Bin isn't installed, so the Windows icons are used"
						: icons === "traybin"
							? "Tray Bin's icons, filling up as the bin grows"
							: icons === "windows"
								? "The standard Windows Recycle Bin icons"
								: "Up to five bars: under 100 MB, 100 MB, 500 MB, 1 GB, 5 GB and up"
				}
			>
				<select
					className="settings-select"
					value={icons}
					onChange={(e) => {
						setIcons(e.target.value);
						save(KEYS.icons, e.target.value);
					}}
				>
					<option value="bloom">Bloom</option>
					<option value="traybin">Tray Bin</option>
					<option value="windows">Windows</option>
				</select>
			</SettingRow>
			{icons !== "windows" && (
				<SettingRow
					icon={LayoutGrid}
					label="Only Empty and Full Icons"
					desc="Skip the in-between icons (the 100 MB, 500 MB, 1 GB and 5 GB steps)"
				>
					<Toggle checked={onlyEmptyFull} onChange={flag(KEYS.onlyEmptyFull, setOnlyEmptyFull)} />
				</SettingRow>
			)}
			<SettingRow
				icon={MousePointerClick}
				label="Double-Click Action"
				desc={
					doubleClick === "empty"
						? "Double-click empties the bin, a single click opens it"
						: "Click or double-click opens the bin"
				}
			>
				<select
					className="settings-select"
					value={doubleClick}
					onChange={(e) => {
						setDoubleClick(e.target.value);
						save(KEYS.doubleClick, e.target.value);
					}}
				>
					<option value="open">Open</option>
					<option value="empty">Empty</option>
				</select>
			</SettingRow>
			<SettingRow icon={ShieldCheck} label="Confirm Recycling" desc="Ask before emptying the bin">
				<Toggle checked={confirm} onChange={flag(KEYS.confirm, setConfirm)} />
			</SettingRow>
			<SettingRow icon={Volume2} label="Allow Sound" desc="Play the Windows sound when emptying">
				<Toggle checked={sound} onChange={flag(KEYS.sound, setSound)} />
			</SettingRow>
			<SettingRow
				icon={Bell}
				label="Allow Progress Window"
				desc="Show progress while a large bin empties"
			>
				<Toggle checked={progress} onChange={flag(KEYS.progress, setProgress)} />
			</SettingRow>
		</>
	);
}
