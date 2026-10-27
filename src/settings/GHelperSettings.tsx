import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Gauge, Keyboard, PanelTop, X } from "lucide-react";
import { SettingRow } from "./SettingRow";
import { HotkeyRecorder } from "./SearchBarTab";
import { useStoredFlag, useStoredString } from "./useStoredSetting";
import type { GHelperBinding, GHelperState } from "../components/GHelperPanel";

/**
 * Settings > Integrations > G-Helper: its page in the notch, and G-Helper's
 * keys (M1 to FN+V) as on its own Key Bindings page, each with a shortcut of
 * your own that does the same from any keyboard.
 */

// G-Helper's actions for its keys, with its own names (Extra.cs).
const KEY_ACTIONS: [string, string][] = [
	["volume_down", "Volume Down"],
	["volume_up", "Volume Up"],
	["backlight_down", "Backlight Down"],
	["backlight_up", "Backlight Up"],
	["mute", "Volume Mute"],
	["screenshot", "PrintScreen"],
	["play", "Play / Pause"],
	["aura", "Toggle Aura"],
	["performance", "Mode"],
	["screen", "Toggle Screen"],
	["lock", "Lock Screen"],
	["miniled", "Toggle Miniled (if supported)"],
	["fnlock", "Toggle Fn-Lock"],
	["brightness_down", "Brightness Down"],
	["brightness_up", "Brightness Up"],
	["visual", "Visual Mode"],
	["touchpad", "Toggle Touchpad"],
	["touchscreen", "Toggle Touchscreen"],
	["micmute", "Mute Mic"],
	["ghelper", "Open G-Helper window"],
	["overlay", "Overlay"],
	["custom", "Custom"]
];

/** What each key does when nothing is chosen, and the entry that leaves out. */
const KEY_DEFAULTS: Record<string, [string, string | null]> = {
	m1: ["Volume Down", "volume_down"],
	m2: ["Volume Up", "volume_up"],
	m3: ["Mute Mic", "micmute"],
	m4: ["Open G-Helper window", "ghelper"],
	fnf4: ["Toggle Aura", "aura"],
	fnc: ["Toggle Fn-Lock", "fnlock"],
	fnv: ["Visual Mode", "visual"]
};

const HOTKEYS_KEY = "bloom-hotkey-ghelper";

interface Shortcut {
	hotkey: string;
	action: string;
}

function parseShortcuts(raw: string): Shortcut[] {
	try {
		const parsed: unknown = JSON.parse(raw || "[]");
		return Array.isArray(parsed)
			? parsed.filter(
					(s): s is Shortcut =>
						!!s && typeof s.hotkey === "string" && typeof s.action === "string"
				)
			: [];
	} catch {
		return [];
	}
}

/** G-Helper does these inside itself only (laptop key or its window). */
const ONLY_IN_GHELPER = ["aura", "visual"];

function BindingRow({
	binding,
	onChange,
	shortcut,
	shortcutTaken,
	onShortcut,
	divider
}: {
	binding: GHelperBinding;
	onChange: (next: GHelperBinding) => void;
	shortcut: string;
	shortcutTaken: boolean;
	onShortcut: (hotkey: string) => void;
	divider: boolean;
}) {
	const [defaultLabel, without] = KEY_DEFAULTS[binding.key] ?? ["--------------", null];
	const effective = binding.action || without || "";
	const insideOnly = ONLY_IN_GHELPER.includes(effective);
	const desc = insideOnly
		? "G-Helper does this one only from the laptop's own key or its window, so a shortcut can't"
		: shortcutTaken
			? "Another app already uses this shortcut. Pick another (or close that app)"
			: undefined;
	return (
		<SettingRow icon={Keyboard} label={binding.label} desc={desc} divider={divider}>
			<div className="sb-inline gh-binding">
				<select
					className="settings-select"
					value={binding.action}
					onChange={(e) => onChange({ ...binding, action: e.target.value })}
				>
					<option value="">{defaultLabel}</option>
					{KEY_ACTIONS.filter(([id]) => id !== without).map(([id, label]) => (
						<option key={id} value={id}>
							{label}
						</option>
					))}
				</select>
				<input
					className="sb-input"
					placeholder="action"
					title="For Custom: the keys to press (hex codes) or a program to run, as in G-Helper"
					value={binding.custom}
					spellCheck={false}
					onChange={(e) => onChange({ ...binding, custom: e.target.value })}
				/>
				<span className="gh-shortcut" title="Your shortcut: does the same from any keyboard">
					{insideOnly ? (
						<button type="button" className="sb-hotkey" disabled>
							Not possible
						</button>
					) : (
						<HotkeyRecorder value={shortcut} onChange={onShortcut} />
					)}
					{shortcut && !insideOnly ? (
						<button
							type="button"
							className="extra-folder-remove"
							title="Remove the shortcut"
							onClick={() => onShortcut("")}
						>
							<X size={13} />
						</button>
					) : (
						<span className="gh-shortcut-space" />
					)}
				</span>
			</div>
		</SettingRow>
	);
}

export function GHelperSettings() {
	const [state, setState] = useState<GHelperState | null>(null);
	const [enabled, setEnabled] = useStoredFlag("bloom-ghelper-enabled", true);
	const [draft, setDraft] = useState<GHelperBinding[] | null>(null);
	const [saving, setSaving] = useState(false);
	const [problem, setProblem] = useState<string | null>(null);
	const [shortcutsRaw, setShortcutsRaw] = useStoredString(HOTKEYS_KEY, "[]");
	const shortcuts = useMemo(() => parseShortcuts(shortcutsRaw), [shortcutsRaw]);
	const [registered, setRegistered] = useState<boolean[]>([]);

	const refresh = useCallback(async () => {
		try {
			setState(await invoke<GHelperState>("ghelper_state", { sensors: false }));
		} catch (e) {
			setProblem(String(e));
		}
	}, []);

	useEffect(() => {
		void refresh();
		const changed = listen("ghelper-changed", () => void refresh());
		return () => {
			void changed.then((f) => f());
		};
	}, [refresh]);

	useEffect(() => {
		const check = () =>
			void invoke<boolean[]>("ghelper_hotkey_status")
				.then(setRegistered)
				.catch(() => {});
		check();
		const changed = listen("app-hotkeys-changed", check);
		return () => {
			void changed.then((f) => f());
		};
	}, []);

	const bindings = draft ?? state?.bindings ?? [];
	const changed =
		!!draft &&
		draft.some((b) => {
			const saved = state?.bindings.find((s) => s.key === b.key);
			return !saved || saved.action !== b.action || saved.custom !== b.custom;
		});

	const saveBindings = async () => {
		if (!draft) return;
		setSaving(true);
		setProblem(null);
		try {
			await invoke("ghelper_set_bindings", { bindings: draft });
			setDraft(null);
			await refresh();
		} catch (e) {
			setProblem(String(e));
		} finally {
			setSaving(false);
		}
	};

	// Each key's own shortcut, stored as { hotkey, action: "key:<key>" }.
	const keyShortcuts = shortcuts.filter((s) => s.action.startsWith("key:"));
	const shortcutFor = (key: string) =>
		keyShortcuts.find((s) => s.action === `key:${key}`)?.hotkey ?? "";
	const shortcutTaken = (key: string) => {
		const index = shortcuts.findIndex((s) => s.action === `key:${key}`);
		return index >= 0 && registered[index] === false;
	};
	// Shortcuts from the first version (a list of their own) are let go of:
	// a shortcut now belongs to one of G-Helper's keys.
	useEffect(() => {
		if (shortcuts.length !== keyShortcuts.length)
			setShortcutsRaw(JSON.stringify(keyShortcuts));
	}, [shortcutsRaw]);
	const setShortcut = (key: string, hotkey: string) => {
		const others = keyShortcuts.filter((s) => s.action !== `key:${key}`);
		const next = hotkey ? [...others, { hotkey, action: `key:${key}` }] : others;
		setShortcutsRaw(JSON.stringify(next));
	};

	const statusText = !state
		? "Looking for G-Helper…"
		: !state.installed
			? "G-Helper wasn't found on this PC. Its page and key bindings show once it is installed"
			: state.running
				? "G-Helper is running. Bloom works through it, so its tuning and settings stay as they are"
				: "G-Helper is installed but not running";

	return (
		<>
			<div className="setting-group-label">G-Helper</div>
			<div className="setting-group">
				<SettingRow icon={Gauge} label="G-Helper" desc={statusText}>
					{state?.installed && !state.running && (
						<button
							type="button"
							className="settings-button"
							onClick={() =>
								void invoke("ghelper_start")
									.then(() => window.setTimeout(() => void refresh(), 2500))
									.catch((e) => setProblem(String(e)))
							}
						>
							Start
						</button>
					)}
				</SettingRow>
				<SettingRow
					icon={PanelTop}
					label="G-Helper in the Notch"
					desc="Mode, GPU mode, screen and battery charge limit in the notch's scroll menu, laid out as in G-Helper"
					divider={false}
				>
					<label className="toggle-switch">
						<input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
						<span className="slider"></span>
					</label>
				</SettingRow>
			</div>

			{state?.installed && (
				<>
					<div className="setting-group-label">Key Bindings</div>
					<div className="setting-group">
						{bindings.map((binding, i) => (
							<BindingRow
								key={binding.key}
								binding={binding}
								divider={i < bindings.length - 1}
								onChange={(next) =>
									setDraft(bindings.map((b) => (b.key === next.key ? next : b)))
								}
								shortcut={shortcutFor(binding.key)}
								shortcutTaken={shortcutTaken(binding.key)}
								onShortcut={(hotkey) => setShortcut(binding.key, hotkey)}
							/>
						))}
					</div>
					<div className="sb-note gh-save-row">
						<span>
							Left: what each laptop key does, as on G-Helper's Key Bindings page (Save
							restarts G-Helper once, in the background). Right: your own shortcut for that
							key. Click it and press any keys on any keyboard; it then does exactly what the
							key does, and follows it when you change the key in G-Helper.
						</span>
						<button
							type="button"
							className="settings-button"
							disabled={!changed || saving}
							onClick={() => void saveBindings()}
						>
							{saving ? "Saving…" : "Save"}
						</button>
						{changed && !saving && (
							<button type="button" className="settings-button" onClick={() => setDraft(null)}>
								Undo
							</button>
						)}
					</div>
				</>
			)}

			{problem && <div className="sb-note">{problem}</div>}
		</>
	);
}
