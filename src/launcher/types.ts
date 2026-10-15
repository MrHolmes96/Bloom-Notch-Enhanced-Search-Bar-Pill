import type { LauncherConfig, PluginId } from "./config";
import type { AppInfo } from "../dockApps";

export type GlyphName =
	| "admin"
	| "ban"
	| "bookmark"
	| "calculator"
	| "copy"
	| "exit"
	| "external"
	| "file"
	| "folder"
	| "gamepad"
	| "globe"
	| "hibernate"
	| "history"
	| "index"
	| "info"
	| "keyword"
	| "link"
	| "lock"
	| "logoff"
	| "open-with"
	| "power"
	| "process"
	| "properties"
	| "refresh"
	| "restart"
	| "search"
	| "settings"
	| "sleep"
	| "star"
	| "terminal"
	| "trash"
	| "warning";

export type Icon =
	| { kind: "app"; path: string; name: string }
	| { kind: "file"; path: string }
	| { kind: "glyph"; glyph: GlyphName };

/** How a result is re-run from history (Ctrl+H, the recent list). */
export type Replay =
	| { type: "app"; path: string; name: string }
	| { type: "path"; path: string; isDir: boolean }
	| { type: "url"; url: string; title: string }
	| { type: "command-line"; command: string }
	| { type: "system"; id: string }
	| { type: "shell"; command: string };

/** An entry in a result's action list (Shift+Enter, right-click). */
export interface ResultAction {
	key: string;
	title: string;
	subtitle?: string;
	glyph: GlyphName;
	run: () => unknown;
	/** Asks for a second Enter first, with this text. */
	confirm?: string;
	keepOpen?: boolean;
}

export interface Result {
	/** Unique and stable: keeps the selection and counts how often it's used. */
	key: string;
	title: string;
	subtitle?: string;
	icon: Icon;
	score: number;
	plugin: PluginId;
	run: () => unknown;
	/** Leave the bar open after running (keyword completion, process kill). */
	keepOpen?: boolean;
	/** Asks for a second Enter first, with this text. */
	confirm?: string;
	/** What Ctrl+Tab puts in the search box. */
	autoComplete?: string;
	/** What Ctrl+C copies when no text is selected. */
	copyText?: string;
	actions?: () => ResultAction[] | Promise<ResultAction[]>;
	/** Ctrl+Enter: open the containing folder. */
	openLocation?: () => unknown;
	/** Ctrl+Shift+Enter. */
	runAsAdmin?: () => unknown;
	preview?: { kind: "file" | "folder" | "app"; path: string };
	replay?: Replay;
	/** Left out of usage counting and history (keyword hints, messages). */
	noHistory?: boolean;
}

export interface DirEntry {
	name: string;
	path: string;
	is_dir: boolean;
}

export interface SearchContext {
	config: LauncherConfig;
	/** Lowest score a fuzzy match needs. */
	min: number;
	/** Score boost from how often a result was picked. */
	boost: (key: string) => number;
	apps: AppInfo[];
	pathPrograms: DirEntry[];
	setQuery: (query: string) => void;
	requery: () => void;
	notify: (message: string) => void;
	/** Clears cached data (F5, "Reload Plugin Data"). */
	reload: () => void;
	/** Opens Bloom's settings on the Search Bar Pill page. */
	openSettings: () => void;
}

/** A parsed query: the keyword (if any) and what was typed after it. */
export interface ParsedQuery {
	raw: string;
	keyword: string | null;
	search: string;
}
