import { invoke } from "@tauri-apps/api/core";
import {
	DEFAULT_ENGINES,
	ENGINE_CATEGORIES,
	REPLACED_SOURCE_IDS,
	providerFor,
	type EngineSettings
} from "./engines";

/**
 * Search Bar Pill settings. The hotkey, position and on/off switch live in
 * their own keys because the backend reads them; everything the search bar
 * page itself uses is one JSON value, "bloom-launcher-config".
 */

export type PluginId =
	| "programs"
	| "explorer"
	| "calculator"
	| "web"
	| "url"
	| "settings"
	| "system"
	| "shell"
	| "processes"
	| "bookmarks"
	| "windows"
	| "commands"
	| "indicator";

export interface PluginInfo {
	id: PluginId;
	name: string;
	description: string;
	/** Flow Launcher's default action keyword; "*" means global. */
	defaultKeyword: string;
	/** Web searches keep a keyword per search engine instead. */
	hasKeyword: boolean;
}

export const PLUGINS: PluginInfo[] = [
	{
		id: "programs",
		name: "Programs",
		description: "Find and open installed apps",
		defaultKeyword: "*",
		hasKeyword: true
	},
	{
		id: "explorer",
		name: "Files & Folders",
		description: "Search files and folders with Windows Search",
		defaultKeyword: "*",
		hasKeyword: true
	},
	{
		id: "calculator",
		name: "Calculator",
		description: "Maths, hex values and functions like sqrt(2) or min(1,2)",
		defaultKeyword: "*",
		hasKeyword: true
	},
	{
		id: "web",
		name: "Web Searches",
		description: "Search the web with a keyword per site",
		defaultKeyword: "*",
		hasKeyword: false
	},
	{
		id: "url",
		name: "URL",
		description: "Open a typed web address",
		defaultKeyword: "*",
		hasKeyword: true
	},
	{
		id: "settings",
		name: "Windows Settings",
		description: "Search Settings and Control Panel",
		defaultKeyword: "s",
		hasKeyword: true
	},
	{
		id: "system",
		name: "System Commands",
		description: "Shut down, lock, sleep, empty the Recycle Bin and more",
		defaultKeyword: "*",
		hasKeyword: true
	},
	{
		id: "shell",
		name: "Shell",
		description: "Run commands in Command Prompt or PowerShell",
		defaultKeyword: ">",
		hasKeyword: true
	},
	{
		id: "processes",
		name: "Process Killer",
		description: "End running processes",
		defaultKeyword: "kill",
		hasKeyword: true
	},
	{
		id: "bookmarks",
		name: "Browser Bookmarks",
		description: "Search bookmarks from Chrome, Edge, Brave, Vivaldi and Opera",
		defaultKeyword: "b",
		hasKeyword: true
	},
	{
		id: "windows",
		name: "Open Windows",
		description: "Switch to any open window by its title or app, like Alt+Tab",
		defaultKeyword: "*",
		hasKeyword: true
	},
	{
		id: "commands",
		name: "Run Commands",
		description: 'What you would type in Run, as Start finds it: "regedit", "dxdiag", "services.msc"',
		defaultKeyword: "*",
		hasKeyword: true
	},
	{
		id: "indicator",
		name: "Plugin Indicator",
		description: "Lists the keywords you can type",
		defaultKeyword: "?",
		hasKeyword: true
	}
];

export interface WebSource {
	id: string;
	title: string;
	/** "*" shows it for every search. */
	keyword: string;
	/** {q} is replaced with the search text. */
	url: string;
	enabled: boolean;
}

export interface QueryShortcut {
	key: string;
	value: string;
}

export interface QueryHotkey {
	hotkey: string;
	query: string;
}

export type Precision = "regular" | "low" | "none";

export interface LauncherConfig {
	width: number;
	maxResults: number;
	lastQuery: "selected" | "preserved" | "empty";
	hideOnBlur: boolean;
	precision: Precision;
	homeHistory: boolean;
	homeHistoryCount: number;
	placeholder: string;
	showClock: boolean;
	animation: boolean;
	alwaysPreview: boolean;
	shortcuts: QueryShortcut[];
	plugins: Record<PluginId, { enabled: boolean; keyword: string }>;
	programs: { showPath: boolean; pathSource: boolean; hidden: string[] };
	explorer: {
		contentKeyword: string;
		contentEnabled: boolean;
		pathSearch: boolean;
		maxResults: number;
		excluded: string[];
		quickAccess: string[];
		excludedTypes: string;
		terminal: "cmd" | "powershell" | "wt";
	};
	calculator: { decimal: "system" | "dot" | "comma"; maxDecimals: number; thousands: boolean };
	/** The search bar's own look; "bloom" follows Settings > Appearance. */
	appearance: { theme: "bloom" | "custom"; color: string; opacity: number };
	web: {
		/** The engine per kind of search (web, images, maps, ...). */
		engines: EngineSettings;
		/** Other sites, each with its own keyword ("youtube lofi"). */
		sources: WebSource[];
		suggestions: boolean;
		suggestionSource: "google" | "bing" | "duckduckgo";
		maxSuggestions: number;
	};
	url: { https: boolean };
	shell: {
		shell: "cmd" | "powershell" | "pwsh" | "run";
		after: "close" | "pause" | "keep";
		terminal: boolean;
		admin: boolean;
		historyCount: number;
	};
	system: { confirm: boolean };
	processes: { showTitle: boolean; windowsFirst: boolean };
	bookmarks: { browsers: string[] };
}

/** Flow Launcher's default site searches, minus the general search engines,
 * which are chosen per kind of search instead (see engines.ts). */
export const DEFAULT_WEB_SOURCES: WebSource[] = [
	["YouTube", "youtube", "https://www.youtube.com/results?search_query={q}"],
	["Netflix", "netflix", "https://www.netflix.com/search?q={q}"],
	["Gmail", "gmail", "https://mail.google.com/mail/u/0/#search/{q}"],
	["Google Drive", "drive", "https://drive.google.com/drive/search?q={q}"],
	["YouTube Music", "ytmusic", "https://music.youtube.com/search?q={q}"],
	["Wikipedia", "wiki", "https://en.wikipedia.org/wiki/Special:Search?search={q}"],
	["Facebook", "facebook", "https://www.facebook.com/search/top/?q={q}"],
	["X", "twitter", "https://x.com/search?q={q}"],
	["GitHub", "github", "https://github.com/search?q={q}"],
	["GitHub Gist", "gist", "https://gist.github.com/search?q={q}"],
	["WolframAlpha", "wolframalpha", "https://www.wolframalpha.com/input/?i={q}"],
	["Stack Overflow", "stackoverflow", "https://stackoverflow.com/search?q={q}"],
	["Reddit", "reddit", "https://www.reddit.com/search/?q={q}"]
].map(([title, keyword, url]) => ({
	id: title.toLowerCase().replace(/[^a-z0-9]+/g, "-"),
	title,
	keyword,
	url,
	enabled: true
}));

export const DEFAULT_CONFIG: LauncherConfig = {
	width: 640,
	maxResults: 6,
	lastQuery: "selected",
	hideOnBlur: true,
	precision: "regular",
	homeHistory: true,
	homeHistoryCount: 5,
	placeholder: "",
	showClock: false,
	animation: true,
	alwaysPreview: false,
	shortcuts: [],
	plugins: Object.fromEntries(
		PLUGINS.map((p) => [p.id, { enabled: true, keyword: p.defaultKeyword }])
	) as LauncherConfig["plugins"],
	programs: { showPath: false, pathSource: false, hidden: [] },
	explorer: {
		contentKeyword: "doc:",
		contentEnabled: true,
		pathSearch: true,
		maxResults: 30,
		excluded: [],
		quickAccess: [],
		excludedTypes: "",
		terminal: "cmd"
	},
	calculator: { decimal: "system", maxDecimals: 10, thousands: true },
	appearance: { theme: "bloom", color: "#1c1c1e", opacity: 0.92 },
	web: {
		engines: DEFAULT_ENGINES,
		sources: DEFAULT_WEB_SOURCES,
		suggestions: false,
		suggestionSource: "duckduckgo",
		maxSuggestions: 3
	},
	url: { https: false },
	shell: { shell: "cmd", after: "keep", terminal: false, admin: false, historyCount: 10 },
	system: { confirm: true },
	processes: { showTitle: true, windowsFirst: false },
	bookmarks: { browsers: ["chrome", "edge", "brave", "vivaldi", "chromium", "opera"] }
};

export const CONFIG_KEY = "bloom-launcher-config";

function isObject(v: unknown): v is Record<string, unknown> {
	return typeof v === "object" && v !== null && !Array.isArray(v);
}

/** Fills in anything missing (older or hand-edited configs) from the defaults. */
function merge<T>(defaults: T, value: unknown): T {
	if (!isObject(defaults) || !isObject(value)) {
		if (value === undefined || value === null) return defaults;
		if (Array.isArray(defaults)) return (Array.isArray(value) ? value : defaults) as T;
		return (typeof value === typeof defaults ? value : defaults) as T;
	}
	const out: Record<string, unknown> = { ...defaults };
	for (const key of Object.keys(defaults)) {
		out[key] = merge((defaults as Record<string, unknown>)[key], value[key]);
	}
	return out as T;
}

export function parseConfig(raw: unknown): LauncherConfig {
	let value: unknown = raw;
	if (typeof raw === "string") {
		try {
			value = JSON.parse(raw);
		} catch {
			value = null;
		}
	}
	const config = merge(DEFAULT_CONFIG, value);
	// Settings from before the per-kind engines: the general engines in the
	// old list are replaced by the new choices (DuckDuckGo by default).
	const stored = isObject(value) && isObject(value.web) ? value.web : null;
	if (stored && !("engines" in stored)) {
		config.web.sources = config.web.sources.filter((s) => !REPLACED_SOURCE_IDS.has(s.id));
		if (config.web.suggestionSource === "google") config.web.suggestionSource = "duckduckgo";
	}
	return config;
}

/** The engines chosen per kind of search, as web sources. */
export function engineSources(config: LauncherConfig): WebSource[] {
	return ENGINE_CATEGORIES.filter((c) => config.web.engines[c.id]?.enabled).map((c) => {
		const setting = config.web.engines[c.id];
		const provider = providerFor(c, setting.provider);
		return {
			id: `engine-${c.id}`,
			title: provider.name,
			keyword: setting.keyword.trim() || c.defaultKeyword,
			url: provider.url,
			enabled: true
		};
	});
}

/** Every web search: the chosen engines first, then the other sites. */
export function allWebSources(config: LauncherConfig): WebSource[] {
	return [...engineSources(config), ...config.web.sources];
}

export function loadConfig(): LauncherConfig {
	return parseConfig(localStorage.getItem(CONFIG_KEY));
}

export function saveConfig(config: LauncherConfig) {
	const value = JSON.stringify(config);
	localStorage.setItem(CONFIG_KEY, value);
	invoke("save_setting", { key: CONFIG_KEY, value }).catch(console.error);
}

/** Settings the backend reads (hotkeys, placement). */
export const BACKEND_KEYS = {
	enabled: "bloom-launcher-enabled",
	hotkey: "bloom-launcher-hotkey",
	ignoreFullscreen: "bloom-launcher-ignore-fullscreen",
	position: "bloom-launcher-position",
	screen: "bloom-launcher-screen",
	queryHotkeys: "bloom-launcher-query-hotkeys"
} as const;

export function saveBackendSetting(key: string, value: string) {
	localStorage.setItem(key, value);
	invoke("save_setting", { key, value }).catch(console.error);
}
