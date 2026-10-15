import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
	AppWindow,
	Bookmark,
	Calculator,
	Clock,
	Command,
	Cpu,
	FolderSearch,
	Gamepad2,
	Globe,
	History,
	Keyboard,
	Link,
	Monitor,
	MousePointerClick,
	MoveVertical,
	PanelRight,
	Pencil,
	Plus,
	Power,
	Puzzle,
	RefreshCw,
	RotateCcw,
	Rows3,
	Ruler,
	Search,
	Settings2,
	Sparkles,
	SquareTerminal,
	Target,
	Trash2,
	Type,
	X
} from "lucide-react";
import { SettingRow } from "./SettingRow";
import { useSettingsSync } from "../hooks/useSettingsSync";
import {
	BACKEND_KEYS,
	CONFIG_KEY,
	DEFAULT_WEB_SOURCES,
	PLUGINS,
	loadConfig,
	parseConfig,
	saveBackendSetting,
	saveConfig,
	type LauncherConfig,
	type QueryHotkey,
	type WebSource
} from "../launcher/config";

function Toggle({ checked, onChange }: { checked: boolean; onChange: (next: boolean) => void }) {
	return (
		<label className="toggle-switch">
			<input type="checkbox" checked={checked} onChange={() => onChange(!checked)} />
			<span className="slider"></span>
		</label>
	);
}

function Select<T extends string>({
	value,
	options,
	onChange
}: {
	value: T;
	options: [T, string][];
	onChange: (v: T) => void;
}) {
	return (
		<select
			className="settings-select"
			value={value}
			onChange={(e) => onChange(e.target.value as T)}
		>
			{options.map(([v, label]) => (
				<option key={v} value={v}>
					{label}
				</option>
			))}
		</select>
	);
}

function TextField({
	value,
	onChange,
	placeholder,
	width = 110
}: {
	value: string;
	onChange: (v: string) => void;
	placeholder?: string;
	width?: number;
}) {
	const [draft, setDraft] = useState(value);
	useEffect(() => setDraft(value), [value]);
	return (
		<input
			className="sb-input"
			style={{ width }}
			value={draft}
			placeholder={placeholder}
			spellCheck={false}
			onChange={(e) => setDraft(e.target.value)}
			onBlur={() => draft !== value && onChange(draft)}
			onKeyDown={(e) => {
				if (e.key === "Enter") (e.target as HTMLInputElement).blur();
				if (e.key === "Escape") setDraft(value);
			}}
		/>
	);
}

const CODE_NAMES: Record<string, string> = {
	Space: "Space",
	Enter: "Enter",
	Tab: "Tab",
	Backspace: "Backspace",
	Insert: "Insert",
	Delete: "Delete",
	Home: "Home",
	End: "End",
	PageUp: "PageUp",
	PageDown: "PageDown",
	ArrowUp: "Up",
	ArrowDown: "Down",
	ArrowLeft: "Left",
	ArrowRight: "Right",
	Backquote: "`",
	Minus: "-",
	Equal: "=",
	BracketLeft: "[",
	BracketRight: "]",
	Backslash: "\\",
	Semicolon: ";",
	Quote: "'",
	Comma: ",",
	Period: ".",
	Slash: "/"
};

function keyName(code: string): string | null {
	if (/^Key[A-Z]$/.test(code)) return code.slice(3);
	if (/^Digit[0-9]$/.test(code)) return code.slice(5);
	if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
	if (/^Numpad[0-9]$/.test(code)) return code;
	return CODE_NAMES[code] ?? null;
}

/** Click, then press the shortcut. The search bar's own shortcuts pause meanwhile. */
function HotkeyRecorder({ value, onChange }: { value: string; onChange: (v: string) => void }) {
	const [recording, setRecording] = useState(false);
	const ref = useRef<HTMLButtonElement>(null);

	useEffect(() => {
		if (!recording) return;
		void invoke("launcher_set_game_mode", { enabled: true });
		return () => {
			void invoke("launcher_set_game_mode", { enabled: false });
		};
	}, [recording]);

	return (
		<button
			ref={ref}
			type="button"
			className={`sb-hotkey ${recording ? "recording" : ""}`}
			onClick={() => setRecording(true)}
			onBlur={() => setRecording(false)}
			onKeyDown={(e) => {
				if (!recording) return;
				e.preventDefault();
				e.stopPropagation();
				if (e.key === "Escape") {
					setRecording(false);
					return;
				}
				const name = keyName(e.code);
				if (!name) return; // a modifier on its own
				const parts = [
					e.ctrlKey && "Ctrl",
					e.altKey && "Alt",
					e.shiftKey && "Shift",
					e.metaKey && "Win",
					name
				].filter(Boolean) as string[];
				if (parts.length === 1 && !/^F\d+$/.test(name)) return;
				onChange(parts.join("+"));
				setRecording(false);
				ref.current?.blur();
			}}
		>
			{recording ? "Press a shortcut…" : value || "Not set"}
		</button>
	);
}

function PathList({
	paths,
	onChange,
	addLabel,
	directory = true
}: {
	paths: string[];
	onChange: (next: string[]) => void;
	addLabel: string;
	directory?: boolean;
}) {
	const add = async () => {
		try {
			const { open } = await import("@tauri-apps/plugin-dialog");
			const picked = await open({ directory, multiple: true, title: addLabel });
			const list = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
			const next = [...paths];
			for (const p of list)
				if (!next.some((x) => x.toLowerCase() === p.toLowerCase())) next.push(p);
			onChange(next);
		} catch (e) {
			console.error("Picker failed:", e);
		}
	};
	return (
		<div className="sb-list">
			{paths.map((path) => (
				<div key={path} className="extra-folder-row">
					<span className="extra-folder-path" title={path}>
						{path}
					</span>
					<button
						type="button"
						className="extra-folder-remove"
						title="Remove"
						onClick={() => onChange(paths.filter((p) => p !== path))}
					>
						<X size={14} strokeWidth={2} />
					</button>
				</div>
			))}
			<button type="button" className="folder-chip add" onClick={add}>
				<Plus size={13} strokeWidth={2} />
				{addLabel}
			</button>
		</div>
	);
}

function Block({
	icon: Icon,
	label,
	desc,
	children
}: {
	icon: typeof Search;
	label: string;
	desc?: string;
	children: React.ReactNode;
}) {
	return (
		<>
			<div className="setting-item sb-block">
				<div className="sb-block-head">
					<div className="setting-icon-bg">
						<Icon size={14} strokeWidth={1.5} />
					</div>
					<div className="setting-info">
						<span className="setting-label">{label}</span>
						{desc && <span className="setting-desc">{desc}</span>}
					</div>
				</div>
				<div className="sb-block-body">{children}</div>
			</div>
			<div className="setting-divider" />
		</>
	);
}

function WebSources({
	sources,
	onChange
}: {
	sources: WebSource[];
	onChange: (next: WebSource[]) => void;
}) {
	const [editing, setEditing] = useState<WebSource | null>(null);
	const blank: WebSource = { id: "", title: "", keyword: "", url: "https://", enabled: true };
	const save = () => {
		if (
			!editing ||
			!editing.title.trim() ||
			!editing.keyword.trim() ||
			!editing.url.includes("{q}")
		)
			return;
		const entry = {
			...editing,
			title: editing.title.trim(),
			keyword: editing.keyword.trim(),
			id:
				editing.id ||
				`${editing.title
					.trim()
					.toLowerCase()
					.replace(/[^a-z0-9]+/g, "-")}-${Date.now()}`
		};
		onChange(
			sources.some((s) => s.id === entry.id)
				? sources.map((s) => (s.id === entry.id ? entry : s))
				: [...sources, entry]
		);
		setEditing(null);
	};
	return (
		<div className="sb-list">
			{sources.map((s) => (
				<div key={s.id} className={`sb-source ${s.enabled ? "" : "off"}`}>
					<input
						type="checkbox"
						checked={s.enabled}
						title={s.enabled ? "Turn off" : "Turn on"}
						onChange={() =>
							onChange(sources.map((x) => (x.id === s.id ? { ...x, enabled: !x.enabled } : x)))
						}
					/>
					<span className="sb-source-title">{s.title}</span>
					<span
						className="sb-keyword"
						title={s.keyword === "*" ? "Shown for every search" : "Keyword"}
					>
						{s.keyword === "*" ? "every search" : s.keyword}
					</span>
					<button
						type="button"
						className="extra-folder-remove"
						title="Edit"
						onClick={() => setEditing({ ...s })}
					>
						<Pencil size={13} strokeWidth={2} />
					</button>
					<button
						type="button"
						className="extra-folder-remove"
						title="Delete"
						onClick={() => onChange(sources.filter((x) => x.id !== s.id))}
					>
						<X size={14} strokeWidth={2} />
					</button>
				</div>
			))}
			{editing ? (
				<div className="sb-editor">
					<input
						className="sb-input"
						placeholder="Name, e.g. Reddit"
						value={editing.title}
						onChange={(e) => setEditing({ ...editing, title: e.target.value })}
					/>
					<input
						className="sb-input"
						placeholder='Keyword, e.g. "r" (or * for every search)'
						value={editing.keyword}
						onChange={(e) => setEditing({ ...editing, keyword: e.target.value })}
					/>
					<input
						className="sb-input"
						placeholder="https://www.reddit.com/search/?q={q}"
						value={editing.url}
						onChange={(e) => setEditing({ ...editing, url: e.target.value })}
					/>
					<span className="setting-desc">{"{q} is replaced with what you search for."}</span>
					<div className="sb-row-buttons">
						<button type="button" className="folder-chip" onClick={() => setEditing(null)}>
							Cancel
						</button>
						<button type="button" className="folder-chip active" onClick={save}>
							Save
						</button>
					</div>
				</div>
			) : (
				<div className="sb-row-buttons">
					<button
						type="button"
						className="folder-chip add"
						onClick={() => setEditing({ ...blank })}
					>
						<Plus size={13} strokeWidth={2} />
						Add search engine
					</button>
					<button
						type="button"
						className="folder-chip"
						onClick={() => onChange(DEFAULT_WEB_SOURCES.map((s) => ({ ...s })))}
					>
						<RotateCcw size={12} strokeWidth={2} />
						Restore defaults
					</button>
				</div>
			)}
		</div>
	);
}

function PairList({
	pairs,
	onChange,
	keyLabel,
	valueLabel,
	addLabel,
	keyIsHotkey = false
}: {
	pairs: { a: string; b: string }[];
	onChange: (next: { a: string; b: string }[]) => void;
	keyLabel: string;
	valueLabel: string;
	addLabel: string;
	keyIsHotkey?: boolean;
}) {
	const [draft, setDraft] = useState<{ a: string; b: string } | null>(null);
	return (
		<div className="sb-list">
			{pairs.map((p, i) => (
				<div key={`${p.a}-${i}`} className="extra-folder-row">
					<span className="sb-keyword">{p.a}</span>
					<span className="extra-folder-path">{p.b}</span>
					<button
						type="button"
						className="extra-folder-remove"
						title="Remove"
						onClick={() => onChange(pairs.filter((_, j) => j !== i))}
					>
						<X size={14} strokeWidth={2} />
					</button>
				</div>
			))}
			{draft ? (
				<div className="sb-editor">
					{keyIsHotkey ? (
						<HotkeyRecorder value={draft.a} onChange={(a) => setDraft({ ...draft, a })} />
					) : (
						<input
							className="sb-input"
							placeholder={keyLabel}
							value={draft.a}
							onChange={(e) => setDraft({ ...draft, a: e.target.value })}
						/>
					)}
					<input
						className="sb-input"
						placeholder={valueLabel}
						value={draft.b}
						onChange={(e) => setDraft({ ...draft, b: e.target.value })}
					/>
					<div className="sb-row-buttons">
						<button type="button" className="folder-chip" onClick={() => setDraft(null)}>
							Cancel
						</button>
						<button
							type="button"
							className="folder-chip active"
							onClick={() => {
								if (draft.a.trim() && draft.b.trim()) {
									onChange([...pairs, { a: draft.a.trim(), b: draft.b }]);
									setDraft(null);
								}
							}}
						>
							Add
						</button>
					</div>
				</div>
			) : (
				<button
					type="button"
					className="folder-chip add"
					onClick={() => setDraft({ a: "", b: "" })}
				>
					<Plus size={13} strokeWidth={2} />
					{addLabel}
				</button>
			)}
		</div>
	);
}

const BROWSERS: [string, string][] = [
	["chrome", "Chrome"],
	["edge", "Edge"],
	["brave", "Brave"],
	["vivaldi", "Vivaldi"],
	["opera", "Opera"],
	["chromium", "Chromium"]
];

const PLUGIN_ICONS: Record<string, typeof Search> = {
	programs: AppWindow,
	explorer: FolderSearch,
	calculator: Calculator,
	web: Globe,
	url: Link,
	settings: Settings2,
	system: Power,
	shell: SquareTerminal,
	processes: Cpu,
	bookmarks: Bookmark,
	indicator: Puzzle
};

function readFlag(key: string, fallback: boolean) {
	const v = localStorage.getItem(key);
	return v === null ? fallback : v === "true";
}

export function SearchBarTab() {
	const [config, setConfigState] = useState<LauncherConfig>(loadConfig);
	const [enabled, setEnabled] = useState(() => readFlag(BACKEND_KEYS.enabled, true));
	const [hotkey, setHotkey] = useState(
		() => localStorage.getItem(BACKEND_KEYS.hotkey) || "Alt+Space"
	);
	const [ignoreFullscreen, setIgnoreFullscreen] = useState(() =>
		readFlag(BACKEND_KEYS.ignoreFullscreen, false)
	);
	const [position, setPosition] = useState(
		() => localStorage.getItem(BACKEND_KEYS.position) || "notch"
	);
	const [screen, setScreen] = useState(() => localStorage.getItem(BACKEND_KEYS.screen) || "cursor");
	const [queryHotkeys, setQueryHotkeys] = useState<QueryHotkey[]>(() => {
		try {
			return JSON.parse(localStorage.getItem(BACKEND_KEYS.queryHotkeys) || "[]");
		} catch {
			return [];
		}
	});
	const [hotkeyOk, setHotkeyOk] = useState<boolean | null>(null);
	const [hiddenNote, setHiddenNote] = useState<string | null>(null);
	const [cleared, setCleared] = useState<"history" | "ranking" | null>(null);
	const flashCleared = (what: "history" | "ranking") => {
		setCleared(what);
		setTimeout(() => setCleared((now) => (now === what ? null : now)), 2500);
	};

	useSettingsSync({ [CONFIG_KEY]: (value) => setConfigState(parseConfig(value)) });

	useEffect(() => {
		void invoke<boolean | null>("launcher_hotkey_status")
			.then(setHotkeyOk)
			.catch(() => {});
		const unlisten = listen<boolean | null>("launcher-hotkey-status", (e) =>
			setHotkeyOk(e.payload)
		);
		return () => {
			unlisten.then((f) => f());
		};
	}, []);

	const update = (change: (draft: LauncherConfig) => void) => {
		const next: LauncherConfig = JSON.parse(JSON.stringify(config));
		change(next);
		setConfigState(next);
		saveConfig(next);
	};

	const setBackend = (key: string, value: string) => saveBackendSetting(key, value);

	return (
		<>
			<div className="setting-group-label">Search Bar Pill</div>
			<div className="setting-group">
				<SettingRow
					icon={Search}
					label="Search Bar"
					desc="A Flow Launcher-style search bar under the notch"
				>
					<Toggle
						checked={enabled}
						onChange={(v) => {
							setEnabled(v);
							setBackend(BACKEND_KEYS.enabled, String(v));
						}}
					/>
				</SettingRow>
				{enabled && (
					<>
						<SettingRow
							icon={Keyboard}
							label="Shortcut"
							desc={
								hotkeyOk === false
									? "Another app already uses this shortcut. Pick another"
									: "Click and press the keys that open the search bar"
							}
						>
							<HotkeyRecorder
								value={hotkey}
								onChange={(v) => {
									setHotkey(v);
									setBackend(BACKEND_KEYS.hotkey, v);
								}}
							/>
						</SettingRow>
						<SettingRow
							icon={Gamepad2}
							label="Ignore in Full Screen"
							desc="The shortcut does nothing while a full-screen app or game is in front"
						>
							<Toggle
								checked={ignoreFullscreen}
								onChange={(v) => {
									setIgnoreFullscreen(v);
									setBackend(BACKEND_KEYS.ignoreFullscreen, String(v));
								}}
							/>
						</SettingRow>
						<SettingRow icon={MoveVertical} label="Position" desc="Where the search bar opens">
							<Select
								value={position}
								options={[
									["notch", "Under the notch"],
									["upper", "Upper third"],
									["center", "Middle"]
								]}
								onChange={(v) => {
									setPosition(v);
									setBackend(BACKEND_KEYS.position, v);
								}}
							/>
						</SettingRow>
						<SettingRow
							icon={Monitor}
							label="Screen"
							desc="Which monitor it opens on"
							divider={false}
						>
							<Select
								value={screen}
								options={[
									["cursor", "With the mouse"],
									["focus", "With the active window"],
									["primary", "Main monitor"]
								]}
								onChange={(v) => {
									setScreen(v);
									setBackend(BACKEND_KEYS.screen, v);
								}}
							/>
						</SettingRow>
					</>
				)}
			</div>

			{enabled && (
				<>
					<div className="setting-group-label">Behaviour</div>
					<div className="setting-group">
						<SettingRow icon={Ruler} label="Width" desc={`${config.width} px`}>
							<input
								type="range"
								min="480"
								max="1000"
								step="20"
								value={config.width}
								className="settings-slider"
								onChange={(e) => update((c) => (c.width = parseInt(e.target.value)))}
							/>
						</SettingRow>
						<SettingRow
							icon={Rows3}
							label="Results Shown"
							desc={`${config.maxResults} rows before it scrolls`}
						>
							<input
								type="range"
								min="3"
								max="12"
								step="1"
								value={config.maxResults}
								className="settings-slider"
								onChange={(e) => update((c) => (c.maxResults = parseInt(e.target.value)))}
							/>
						</SettingRow>
						<SettingRow
							icon={RotateCcw}
							label="When Reopened"
							desc="What happens to your last search"
						>
							<Select
								value={config.lastQuery}
								options={[
									["selected", "Keep it, selected"],
									["preserved", "Keep it"],
									["empty", "Start empty"]
								]}
								onChange={(v) => update((c) => (c.lastQuery = v))}
							/>
						</SettingRow>
						<SettingRow
							icon={MousePointerClick}
							label="Close When Clicking Away"
							desc="Hide the search bar when another window gets focus"
						>
							<Toggle
								checked={config.hideOnBlur}
								onChange={(v) => update((c) => (c.hideOnBlur = v))}
							/>
						</SettingRow>
						<SettingRow icon={Target} label="Search Precision" desc="How loose a match can be">
							<Select
								value={config.precision}
								options={[
									["regular", "Regular"],
									["low", "Low"],
									["none", "None"]
								]}
								onChange={(v) => update((c) => (c.precision = v))}
							/>
						</SettingRow>
						<SettingRow
							icon={History}
							label="Recent Items"
							desc="Show what you opened last before you type (Ctrl+H for all)"
						>
							<Select
								value={config.homeHistory ? String(config.homeHistoryCount) : "off"}
								options={[
									["off", "Off"],
									["3", "3"],
									["5", "5"],
									["8", "8"],
									["12", "12"]
								]}
								onChange={(v) =>
									update((c) => {
										c.homeHistory = v !== "off";
										if (v !== "off") c.homeHistoryCount = parseInt(v);
									})
								}
							/>
						</SettingRow>
						<SettingRow
							icon={PanelRight}
							label="Always Show Preview"
							desc="File details beside the results (F1 toggles)"
						>
							<Toggle
								checked={config.alwaysPreview}
								onChange={(v) => update((c) => (c.alwaysPreview = v))}
							/>
						</SettingRow>
						<SettingRow icon={Clock} label="Clock" desc="Show the time in the empty search box">
							<Toggle
								checked={config.showClock}
								onChange={(v) => update((c) => (c.showClock = v))}
							/>
						</SettingRow>
						<SettingRow
							icon={Sparkles}
							label="Animation"
							desc="Fade the search bar in when it opens"
						>
							<Toggle
								checked={config.animation}
								onChange={(v) => update((c) => (c.animation = v))}
							/>
						</SettingRow>
						<SettingRow
							icon={Type}
							label="Placeholder"
							desc="Text shown in the empty search box"
							divider={false}
						>
							<TextField
								value={config.placeholder}
								placeholder="Default"
								width={150}
								onChange={(v) => update((c) => (c.placeholder = v))}
							/>
						</SettingRow>
					</div>

					<div className="setting-group-label">Plugins & Keywords</div>
					<div className="setting-group">
						{PLUGINS.map((plugin, i) => {
							const p = config.plugins[plugin.id];
							const Icon = PLUGIN_ICONS[plugin.id] ?? Puzzle;
							return (
								<SettingRow
									key={plugin.id}
									icon={Icon}
									label={plugin.name}
									desc={plugin.description}
									divider={i < PLUGINS.length - 1}
								>
									<div className="sb-inline">
										{plugin.hasKeyword && p.enabled && (
											<TextField
												value={p.keyword}
												width={54}
												placeholder="*"
												onChange={(v) =>
													update((c) => (c.plugins[plugin.id].keyword = v.trim() || "*"))
												}
											/>
										)}
										<Toggle
											checked={p.enabled}
											onChange={(v) => update((c) => (c.plugins[plugin.id].enabled = v))}
										/>
									</div>
								</SettingRow>
							);
						})}
					</div>
					<div className="sb-note">
						A keyword limits the search to one plugin: type it, then a space ("kill chrome"). *
						means the plugin joins every search.
					</div>

					<div className="setting-group-label">Programs</div>
					<div className="setting-group">
						<SettingRow
							icon={AppWindow}
							label="Show App Path"
							desc="Show where each app lives under its name"
						>
							<Toggle
								checked={config.programs.showPath}
								onChange={(v) => update((c) => (c.programs.showPath = v))}
							/>
						</SettingRow>
						<SettingRow
							icon={SquareTerminal}
							label="Include Programs on PATH"
							desc="Also find command-line tools such as git or python"
						>
							<Toggle
								checked={config.programs.pathSource}
								onChange={(v) => update((c) => (c.programs.pathSource = v))}
							/>
						</SettingRow>
						{config.programs.hidden.length > 0 && (
							<Block icon={X} label="Hidden Programs" desc="Hidden with Hide from results">
								<PathList
									paths={config.programs.hidden}
									addLabel=""
									onChange={(next) => update((c) => (c.programs.hidden = next))}
								/>
							</Block>
						)}
						<SettingRow
							icon={RefreshCw}
							label="Reload Search Data"
							desc={hiddenNote ?? "Rescan installed apps now (F5 in the search bar)"}
							action
							divider={false}
							onClick={() => {
								void invoke("launcher_reindex");
								setHiddenNote("Rescanning apps…");
								setTimeout(() => setHiddenNote(null), 2500);
							}}
						/>
					</div>

					<div className="setting-group-label">Files & Folders</div>
					<div className="setting-group">
						<SettingRow
							icon={FolderSearch}
							label="Search Inside Files"
							desc={`Type "${config.explorer.contentKeyword}" and a word to search file contents`}
						>
							<div className="sb-inline">
								{config.explorer.contentEnabled && (
									<TextField
										value={config.explorer.contentKeyword}
										width={54}
										onChange={(v) =>
											update((c) => (c.explorer.contentKeyword = v.trim() || "doc:"))
										}
									/>
								)}
								<Toggle
									checked={config.explorer.contentEnabled}
									onChange={(v) => update((c) => (c.explorer.contentEnabled = v))}
								/>
							</div>
						</SettingRow>
						<SettingRow
							icon={Link}
							label="Path Search"
							desc={'Type a path such as "C:\\Users\\" to browse it'}
						>
							<Toggle
								checked={config.explorer.pathSearch}
								onChange={(v) => update((c) => (c.explorer.pathSearch = v))}
							/>
						</SettingRow>
						<SettingRow
							icon={Rows3}
							label="Maximum Results"
							desc={`${config.explorer.maxResults} files per search`}
						>
							<input
								type="range"
								min="5"
								max="100"
								step="5"
								value={config.explorer.maxResults}
								className="settings-slider"
								onChange={(e) => update((c) => (c.explorer.maxResults = parseInt(e.target.value)))}
							/>
						</SettingRow>
						<SettingRow
							icon={SquareTerminal}
							label="Open in Terminal"
							desc="Used by the Open in terminal action"
						>
							<Select
								value={config.explorer.terminal}
								options={[
									["cmd", "Command Prompt"],
									["powershell", "PowerShell"],
									["wt", "Windows Terminal"]
								]}
								onChange={(v) => update((c) => (c.explorer.terminal = v))}
							/>
						</SettingRow>
						<SettingRow icon={X} label="Excluded File Types" desc="Comma-separated, e.g. tmp, log">
							<TextField
								value={config.explorer.excludedTypes}
								width={130}
								placeholder="None"
								onChange={(v) => update((c) => (c.explorer.excludedTypes = v))}
							/>
						</SettingRow>
						<Block
							icon={Bookmark}
							label="Quick Access"
							desc="Folders and files that always show first"
						>
							<PathList
								paths={config.explorer.quickAccess}
								addLabel="Add folder…"
								onChange={(next) => update((c) => (c.explorer.quickAccess = next))}
							/>
						</Block>
						<Block icon={X} label="Excluded Folders" desc="Never show anything from these">
							<PathList
								paths={config.explorer.excluded}
								addLabel="Add folder…"
								onChange={(next) => update((c) => (c.explorer.excluded = next))}
							/>
						</Block>
						<SettingRow
							icon={Settings2}
							label="Windows Indexing Options"
							desc="Choose which folders Windows Search covers"
							action
							divider={false}
							onClick={() =>
								void invoke("launcher_system_command", { command: "indexing-options" })
							}
						/>
					</div>

					<div className="setting-group-label">Calculator</div>
					<div className="setting-group">
						<SettingRow
							icon={Calculator}
							label="Decimal Separator"
							desc="How you type and see decimals"
						>
							<Select
								value={config.calculator.decimal}
								options={[
									["system", "Windows setting"],
									["dot", "Dot (1.5)"],
									["comma", "Comma (1,5)"]
								]}
								onChange={(v) => update((c) => (c.calculator.decimal = v))}
							/>
						</SettingRow>
						<SettingRow
							icon={Rows3}
							label="Decimal Places"
							desc={`Round results to ${config.calculator.maxDecimals} places`}
						>
							<input
								type="range"
								min="0"
								max="15"
								step="1"
								value={config.calculator.maxDecimals}
								className="settings-slider"
								onChange={(e) =>
									update((c) => (c.calculator.maxDecimals = parseInt(e.target.value)))
								}
							/>
						</SettingRow>
						<SettingRow
							icon={Type}
							label="Thousands Separator"
							desc="Show 1,234,567 instead of 1234567"
							divider={false}
						>
							<Toggle
								checked={config.calculator.thousands}
								onChange={(v) => update((c) => (c.calculator.thousands = v))}
							/>
						</SettingRow>
					</div>

					<div className="setting-group-label">Web Searches</div>
					<div className="setting-group">
						<Block
							icon={Globe}
							label="Search Engines"
							desc='Type the keyword and a search ("youtube lofi")'
						>
							<WebSources
								sources={config.web.sources}
								onChange={(next) => update((c) => (c.web.sources = next))}
							/>
						</Block>
						<SettingRow
							icon={Sparkles}
							label="Search Suggestions"
							desc="Ask the engine below for suggestions as you type (sends what you type)"
						>
							<Toggle
								checked={config.web.suggestions}
								onChange={(v) => update((c) => (c.web.suggestions = v))}
							/>
						</SettingRow>
						{config.web.suggestions && (
							<SettingRow
								icon={Globe}
								label="Suggestions From"
								desc={`Up to ${config.web.maxSuggestions}`}
								divider={false}
							>
								<Select
									value={config.web.suggestionSource}
									options={[
										["google", "Google"],
										["bing", "Bing"],
										["duckduckgo", "DuckDuckGo"]
									]}
									onChange={(v) => update((c) => (c.web.suggestionSource = v))}
								/>
							</SettingRow>
						)}
						{!config.web.suggestions && <div className="sb-group-end" />}
					</div>

					<div className="setting-group-label">URL & Bookmarks</div>
					<div className="setting-group">
						<SettingRow
							icon={Link}
							label="Always Use HTTPS"
							desc='Open "example.com" as https://example.com'
						>
							<Toggle
								checked={config.url.https}
								onChange={(v) => update((c) => (c.url.https = v))}
							/>
						</SettingRow>
						<Block
							icon={Bookmark}
							label="Bookmarks From"
							desc={`Type "${config.plugins.bookmarks.keyword}" and a name`}
						>
							<div className="sb-chips">
								{BROWSERS.map(([id, name]) => {
									const on = config.bookmarks.browsers.includes(id);
									return (
										<button
											key={id}
											type="button"
											className={`folder-chip ${on ? "active" : ""}`}
											onClick={() =>
												update((c) => {
													c.bookmarks.browsers = on
														? c.bookmarks.browsers.filter((b) => b !== id)
														: [...c.bookmarks.browsers, id];
												})
											}
										>
											{name}
										</button>
									);
								})}
							</div>
						</Block>
					</div>

					<div className="setting-group-label">Shell</div>
					<div className="setting-group">
						<SettingRow
							icon={SquareTerminal}
							label="Run Commands In"
							desc={`Type "${config.plugins.shell.keyword}" and a command`}
						>
							<Select
								value={config.shell.shell}
								options={[
									["cmd", "Command Prompt"],
									["powershell", "PowerShell"],
									["pwsh", "PowerShell 7"],
									["run", "Run dialog"]
								]}
								onChange={(v) => update((c) => (c.shell.shell = v))}
							/>
						</SettingRow>
						{config.shell.shell !== "run" && (
							<>
								<SettingRow
									icon={Command}
									label="After Running"
									desc="What the window does when the command ends"
								>
									<Select
										value={config.shell.after}
										options={[
											["keep", "Stay open"],
											["pause", "Wait for a key"],
											["close", "Close"]
										]}
										onChange={(v) => update((c) => (c.shell.after = v))}
									/>
								</SettingRow>
								<SettingRow
									icon={SquareTerminal}
									label="Use Windows Terminal"
									desc="Open commands in a Windows Terminal tab"
								>
									<Toggle
										checked={config.shell.terminal}
										onChange={(v) => update((c) => (c.shell.terminal = v))}
									/>
								</SettingRow>
							</>
						)}
						<SettingRow
							icon={Settings2}
							label="Always as Administrator"
							desc="Otherwise Ctrl+Shift+Enter runs one as administrator"
							divider={false}
						>
							<Toggle
								checked={config.shell.admin}
								onChange={(v) => update((c) => (c.shell.admin = v))}
							/>
						</SettingRow>
					</div>

					<div className="setting-group-label">System & Processes</div>
					<div className="setting-group">
						<SettingRow
							icon={Power}
							label="Confirm System Commands"
							desc="Press Enter twice to shut down, restart, sign out or exit Bloom"
						>
							<Toggle
								checked={config.system.confirm}
								onChange={(v) => update((c) => (c.system.confirm = v))}
							/>
						</SettingRow>
						<SettingRow
							icon={Cpu}
							label="Show Window Titles"
							desc={`In the process list ("${config.plugins.processes.keyword}")`}
						>
							<Toggle
								checked={config.processes.showTitle}
								onChange={(v) => update((c) => (c.processes.showTitle = v))}
							/>
						</SettingRow>
						<SettingRow
							icon={AppWindow}
							label="Apps With Windows First"
							desc="List processes that have an open window at the top"
							divider={false}
						>
							<Toggle
								checked={config.processes.windowsFirst}
								onChange={(v) => update((c) => (c.processes.windowsFirst = v))}
							/>
						</SettingRow>
					</div>

					<div className="setting-group-label">Shortcuts</div>
					<div className="setting-group">
						<Block
							icon={Type}
							label="Query Shortcuts"
							desc='Short text that expands, e.g. "gh" → "github ". {clipboard} inserts the clipboard'
						>
							<PairList
								pairs={config.shortcuts.map((s) => ({ a: s.key, b: s.value }))}
								keyLabel="Shortcut, e.g. gh"
								valueLabel="Expands to, e.g. github"
								addLabel="Add shortcut"
								onChange={(next) =>
									update((c) => (c.shortcuts = next.map((p) => ({ key: p.a, value: p.b }))))
								}
							/>
						</Block>
						<Block
							icon={Keyboard}
							label="Query Hotkeys"
							desc="A shortcut that opens the search bar with a search typed in"
						>
							<PairList
								pairs={queryHotkeys.map((h) => ({ a: h.hotkey, b: h.query }))}
								keyLabel="Shortcut"
								valueLabel="Search, e.g. > or kill "
								addLabel="Add query hotkey"
								keyIsHotkey
								onChange={(next) => {
									const list = next.map((p) => ({ hotkey: p.a, query: p.b }));
									setQueryHotkeys(list);
									setBackend(BACKEND_KEYS.queryHotkeys, JSON.stringify(list));
								}}
							/>
						</Block>
					</div>

					<div className="setting-group-label">History</div>
					<div className="setting-group">
						<SettingRow
							icon={Trash2}
							label="Clear History"
							desc={
								cleared === "history"
									? "History cleared"
									: "Forget recent items, past searches and shell commands"
							}
							action
							danger
							onClick={() => {
								for (const key of [
									"launcher-history",
									"launcher-queries",
									"launcher-shell-history"
								]) {
									localStorage.removeItem(key);
								}
								flashCleared("history");
							}}
						/>
						<SettingRow
							icon={RotateCcw}
							label="Reset Ranking"
							desc={
								cleared === "ranking" ? "Ranking reset" : "Forget which results you pick most often"
							}
							action
							danger
							divider={false}
							onClick={() => {
								localStorage.removeItem("launcher-usage");
								flashCleared("ranking");
							}}
						/>
					</div>
				</>
			)}
		</>
	);
}
