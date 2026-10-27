import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Gauge, Keyboard, PanelTop, Plus, X } from "lucide-react";
import { SettingRow } from "./SettingRow";
import { HotkeyRecorder } from "./SearchBarTab";
import { useStoredFlag, useStoredString } from "./useStoredSetting";
import type { GHelperBinding, GHelperState } from "../components/GHelperPanel";

/**
 * Settings > Integrations > G-Helper: its page in the notch, what G-Helper's
 * keys (M1 to FN+V) do, as on G-Helper's own Key Bindings page, and Bloom
 * shortcuts from any keyboard for G-Helper's controls.
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

/** What a Bloom shortcut can do, grouped as in the notch. */
function shortcutActions(state: GHelperState | null): { group: string; items: [string, string][] }[] {
	const modes = state?.modes ?? [
		{ id: 2, name: "Silent", base: 2 },
		{ id: 0, name: "Balanced", base: 0 },
		{ id: 1, name: "Turbo", base: 1 }
	];
	return [
		{
			group: "Mode",
			items: [
				...modes.map((m): [string, string] => [`mode:${m.id}`, `Mode: ${m.name}`]),
				["mode:cycle", "Mode: next"]
			]
		},
		{
			group: "GPU Mode",
			items: [
				["gpu:eco", "GPU: Eco"],
				["gpu:standard", "GPU: Standard"],
				["gpu:optimized", "GPU: Optimized"]
			]
		},
		{
			group: "Screen",
			items: [
				["screen:auto", "Screen: Auto"],
				["screen:min", `Screen: ${state?.refresh_min ?? 60}Hz`],
				["screen:max", `Screen: ${state && state.refresh_max > 0 ? state.refresh_max : "highest "}Hz`],
				["screen:toggle", "Screen: switch refresh rate"],
				["miniled", "Screen: Multi Zone on/off"]
			]
		},
		{
			group: "Battery",
			items: [60, 70, 80, 90, 100].map((n): [string, string] => [`charge:${n}`, `Charge limit ${n}%`])
		},
		{
			group: "Other",
			items: [
				["touchscreen", "Toggle Touchscreen"],
				["micmute", "Mute Mic"],
				["open", "Open G-Helper window"]
			]
		}
	];
}

function BindingRow({
	binding,
	onChange,
	divider
}: {
	binding: GHelperBinding;
	onChange: (next: GHelperBinding) => void;
	divider: boolean;
}) {
	const [defaultLabel, without] = KEY_DEFAULTS[binding.key] ?? ["--------------", null];
	return (
		<SettingRow icon={Keyboard} label={binding.label} divider={divider}>
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
	const [adding, setAdding] = useState<Shortcut | null>(null);

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

	const actions = shortcutActions(state);
	const actionLabel = (id: string) =>
		actions.flatMap((g) => g.items).find(([value]) => value === id)?.[1] ?? id;
	const saveShortcuts = (next: Shortcut[]) => setShortcutsRaw(JSON.stringify(next));

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
							/>
						))}
					</div>
					<div className="sb-note gh-save-row">
						<span>
							The laptop's own keys, as on G-Helper's Key Bindings page. Saving restarts
							G-Helper once (2–3 seconds) so it takes them.
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

			<div className="setting-group-label">Shortcuts</div>
			<div className="setting-group">
				{shortcuts.length === 0 && !adding && (
					<SettingRow
						icon={Keyboard}
						label="No shortcuts yet"
						desc="Add one for any G-Helper control, from any keyboard"
						divider={false}
					/>
				)}
				{shortcuts.map((shortcut, i) => (
					<SettingRow
						key={`${shortcut.hotkey}-${i}`}
						icon={Keyboard}
						label={actionLabel(shortcut.action)}
						desc={
							registered[i] === false
								? "Another app already uses this shortcut. Pick another (or close that app)"
								: undefined
						}
						divider={i < shortcuts.length - 1 || !!adding}
					>
						<div className="sb-inline">
							<HotkeyRecorder
								value={shortcut.hotkey}
								onChange={(hotkey) =>
									saveShortcuts(shortcuts.map((s, j) => (j === i ? { ...s, hotkey } : s)))
								}
							/>
							<button
								type="button"
								className="extra-folder-remove"
								title="Remove the shortcut"
								onClick={() => saveShortcuts(shortcuts.filter((_, j) => j !== i))}
							>
								<X size={13} />
							</button>
						</div>
					</SettingRow>
				))}
				{adding && (
					<SettingRow icon={Plus} label="New Shortcut" divider={false}>
						<div className="sb-inline gh-binding">
							<select
								className="settings-select"
								value={adding.action}
								onChange={(e) => setAdding({ ...adding, action: e.target.value })}
							>
								{actions.map(({ group, items }) => (
									<optgroup key={group} label={group}>
										{items.map(([value, label]) => (
											<option key={value} value={value}>
												{label}
											</option>
										))}
									</optgroup>
								))}
							</select>
							<HotkeyRecorder
								value={adding.hotkey}
								onChange={(hotkey) => setAdding({ ...adding, hotkey })}
							/>
						</div>
					</SettingRow>
				)}
			</div>
			<div className="sb-note gh-save-row">
				<span>
					Click the shortcut box, then press the keys: a combination, or a single key from F13 to
					F24 (which a Logitech key can send through OpenLogi). Toggle Touchscreen asks Windows
					for permission once the first time.
				</span>
				{adding ? (
					<>
						<button type="button" className="settings-button" onClick={() => setAdding(null)}>
							Cancel
						</button>
						<button
							type="button"
							className="settings-button"
							disabled={!adding.hotkey}
							onClick={() => {
								saveShortcuts([...shortcuts, adding]);
								setAdding(null);
							}}
						>
							Add
						</button>
					</>
				) : (
					<button
						type="button"
						className="settings-button"
						onClick={() => setAdding({ hotkey: "", action: "touchscreen" })}
					>
						Add Shortcut
					</button>
				)}
			</div>
			{problem && <div className="sb-note">{problem}</div>}
		</>
	);
}
