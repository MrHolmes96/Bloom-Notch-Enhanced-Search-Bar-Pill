import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Gauge, Keyboard, PanelTop } from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useStoredFlag } from "./useStoredSetting";
import type { GHelperBinding, GHelperState } from "../components/GHelperPanel";

/**
 * Settings > Integrations > G-Helper: its page in the notch, and G-Helper's
 * keys (M1 to FN+V) as on its own Key Bindings page. For "Custom", the keys
 * to press are recorded by pressing them, and written in G-Helper's own
 * format (Windows key codes, "0x11 0x10 0x31"), so G-Helper presses them.
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

// Windows' virtual-key codes, as G-Helper's Custom action takes them.
const MODIFIERS: ["ctrlKey" | "shiftKey" | "altKey" | "metaKey", number, string][] = [
	["ctrlKey", 0x11, "Ctrl"],
	["shiftKey", 0x10, "Shift"],
	["altKey", 0x12, "Alt"],
	["metaKey", 0x5b, "Win"]
];

const NAMED_KEYS: Record<string, [number, string]> = {
	Space: [0x20, "Space"],
	Enter: [0x0d, "Enter"],
	NumpadEnter: [0x0d, "Enter"],
	Tab: [0x09, "Tab"],
	Backspace: [0x08, "Backspace"],
	Insert: [0x2d, "Insert"],
	Delete: [0x2e, "Delete"],
	Home: [0x24, "Home"],
	End: [0x23, "End"],
	PageUp: [0x21, "Page Up"],
	PageDown: [0x22, "Page Down"],
	ArrowLeft: [0x25, "Left"],
	ArrowUp: [0x26, "Up"],
	ArrowRight: [0x27, "Right"],
	ArrowDown: [0x28, "Down"],
	PrintScreen: [0x2c, "PrintScreen"],
	Pause: [0x13, "Pause"],
	CapsLock: [0x14, "Caps Lock"],
	ScrollLock: [0x91, "Scroll Lock"],
	NumLock: [0x90, "Num Lock"],
	ContextMenu: [0x5d, "Menu"],
	NumpadMultiply: [0x6a, "Num *"],
	NumpadAdd: [0x6b, "Num +"],
	NumpadSubtract: [0x6d, "Num -"],
	NumpadDecimal: [0x6e, "Num ."],
	NumpadDivide: [0x6f, "Num /"],
	Semicolon: [0xba, ";"],
	Equal: [0xbb, "="],
	Comma: [0xbc, ","],
	Minus: [0xbd, "-"],
	Period: [0xbe, "."],
	Slash: [0xbf, "/"],
	Backquote: [0xc0, "`"],
	BracketLeft: [0xdb, "["],
	Backslash: [0xdc, "\\"],
	BracketRight: [0xdd, "]"],
	Quote: [0xde, "'"],
	IntlBackslash: [0xe2, "<"],
	AudioVolumeMute: [0xad, "Mute"],
	AudioVolumeDown: [0xae, "Volume Down"],
	AudioVolumeUp: [0xaf, "Volume Up"],
	MediaTrackNext: [0xb0, "Next Track"],
	MediaTrackPrevious: [0xb1, "Previous Track"],
	MediaStop: [0xb2, "Stop"],
	MediaPlayPause: [0xb3, "Play / Pause"]
};

/** The left and right versions of the modifiers, as G-Helper's docs use them. */
const SIDE_MODIFIERS: Record<number, string> = {
	0xa0: "Shift",
	0xa1: "Shift",
	0xa2: "Ctrl",
	0xa3: "Ctrl",
	0xa4: "Alt",
	0xa5: "Alt",
	0x5c: "Win"
};

/** A pressed key (not a modifier): its code and name. */
function keyFor(code: string): [number, string] | null {
	if (/^Key[A-Z]$/.test(code)) return [code.charCodeAt(3), code.slice(3)];
	if (/^Digit[0-9]$/.test(code)) return [code.charCodeAt(5), code.slice(5)];
	const f = /^F([1-9]|1[0-9]|2[0-4])$/.exec(code);
	if (f) return [0x6f + Number(f[1]), code];
	const num = /^Numpad([0-9])$/.exec(code);
	if (num) return [0x60 + Number(num[1]), `Num ${num[1]}`];
	return NAMED_KEYS[code] ?? null;
}

const hex = (code: number) => `0x${code.toString(16).toUpperCase().padStart(2, "0")}`;

function codeName(code: number): string {
	const modifier = MODIFIERS.find(([, c]) => c === code)?.[2] ?? SIDE_MODIFIERS[code];
	if (modifier) return modifier;
	const named = Object.values(NAMED_KEYS).find(([c]) => c === code)?.[1];
	if (named) return named;
	if ((code >= 0x30 && code <= 0x39) || (code >= 0x41 && code <= 0x5a))
		return String.fromCharCode(code);
	if (code >= 0x70 && code <= 0x87) return `F${code - 0x6f}`;
	if (code >= 0x60 && code <= 0x69) return `Num ${code - 0x60}`;
	return hex(code);
}

/** "0x11 0x10 0x31" as "Ctrl+Shift+1", when it is a list of key codes. */
function describeCodes(text: string): string | null {
	const parts = text.trim().split(/\s+/).filter(Boolean);
	if (parts.length === 0 || parts.length > 4) return null;
	if (!parts.every((part) => /^(0x)?[0-9a-f]{1,2}$/i.test(part))) return null;
	return parts.map((part) => codeName(parseInt(part.replace(/^0x/i, ""), 16))).join("+");
}

/**
 * Click, then press the keys. They are written as G-Helper's Custom action
 * takes them: Windows key codes separated by spaces, at most four keys.
 */
function KeysRecorder({ onKeys }: { onKeys: (codes: string) => void }) {
	const [recording, setRecording] = useState(false);
	const [note, setNote] = useState<string | null>(null);
	const ref = useRef<HTMLButtonElement>(null);

	useEffect(() => {
		if (!recording) return;
		// Bloom's own shortcuts and the search bar's wait meanwhile.
		void invoke("launcher_set_game_mode", { enabled: true });
		void invoke("set_hotkey_recording", { recording: true });
		return () => {
			void invoke("launcher_set_game_mode", { enabled: false });
			void invoke("set_hotkey_recording", { recording: false });
		};
	}, [recording]);

	const take = (e: React.KeyboardEvent) => {
		const key = keyFor(e.code);
		if (!key) return;
		const codes = MODIFIERS.filter(([flag]) => e[flag]).map(([, code]) => code);
		codes.push(key[0]);
		if (codes.length > 4) {
			setNote("G-Helper takes at most four keys");
			return;
		}
		onKeys(codes.map(hex).join(" "));
		setNote(null);
		setRecording(false);
		ref.current?.blur();
	};

	return (
		<button
			ref={ref}
			type="button"
			className={`sb-hotkey gh-record${recording ? " recording" : ""}`}
			title="Click, then press the keys G-Helper should press"
			onClick={() => {
				setNote(null);
				setRecording(true);
			}}
			onBlur={() => setRecording(false)}
			onKeyDown={(e) => {
				if (!recording) return;
				e.preventDefault();
				e.stopPropagation();
				if (e.code === "Escape" && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) {
					setRecording(false);
					return;
				}
				take(e);
			}}
			onKeyUp={(e) => {
				// Print Screen only ever arrives as its key going up.
				if (recording && e.code === "PrintScreen") {
					e.preventDefault();
					take(e);
				}
			}}
		>
			{recording ? (note ?? "Press the keys…") : "Record keys"}
		</button>
	);
}

/** A Custom command an earlier Bloom set up (its own touchscreen switch). */
const isOldBloomCommand = (text: string) =>
	/bloom[^"]*\.exe"?\s+--touchscreen\s*$/i.test(text.trim());

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
	const custom = binding.action === "custom";
	const described = custom ? describeCodes(binding.custom) : null;
	return (
		<SettingRow
			icon={Keyboard}
			label={binding.label}
			desc={described ? `Presses ${described}` : undefined}
			divider={divider}
		>
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
					title="For Custom: the keys to press (Windows key codes) or a program to run, as in G-Helper"
					value={binding.custom}
					spellCheck={false}
					onChange={(e) => onChange({ ...binding, custom: e.target.value })}
				/>
				{/* Recording sets the key to Custom with the keys pressed. */}
				<KeysRecorder
					onKeys={(codes) => onChange({ ...binding, action: "custom", custom: codes })}
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

	// A key an earlier Bloom pointed at its own touchscreen switch goes back
	// to G-Helper's Toggle Touchscreen, shown as a change to save.
	useEffect(() => {
		if (!state || draft) return;
		if (state.bindings.some((b) => b.action === "custom" && isOldBloomCommand(b.custom)))
			setDraft(
				state.bindings.map((b) =>
					b.action === "custom" && isOldBloomCommand(b.custom)
						? { ...b, action: "touchscreen", custom: "" }
						: b
				)
			);
	}, [state]);

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
							G-Helper's own Key Bindings. Pick an action, or click Record keys on a row and
							press any keys: that key is set to Custom and G-Helper presses them for it.
							Save writes it all into G-Helper (it restarts once in the background).
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
