import { allWebSources, type LauncherConfig, type PluginId } from "./config";
import { minScore, scoreMatch } from "./fuzzy";
import { forgetHistory, recentHistory, type HistoryEntry } from "./history";
import { open, openApp, runCommandLine } from "./ops";
import { searchIndex, searchPath, searchQuickAccess, isLocationPath } from "./plugins/files";
import { searchPrograms } from "./plugins/programs";
import { runShell, searchProcesses, searchShell } from "./plugins/shell";
import {
	SYSTEM_COMMANDS,
	searchCalculator,
	searchIndicator,
	searchSystem,
	searchWindowsSettings
} from "./plugins/system";
import { searchBookmarks, searchUrl, searchWeb, webSuggestions } from "./plugins/web";
import { searchWindows } from "./plugins/windows";
import { searchCommands } from "./plugins/commands";
import type { ParsedQuery, Replay, Result, SearchContext } from "./types";

/** What a keyword switches the bar to. */
type Target = {
	plugin: PluginId;
	mode?: "content" | "kind";
	/** Start's file filters: "documents", "folders", "photos", "videos", "music". */
	kind?: string;
	/** Start's "web:" filter: the engines offered under every search. */
	anyEngine?: boolean;
};

/**
 * Start search's filters, typed before a search the same way: "apps: word",
 * "documents: budget", "photos: beach". Each searches only that kind.
 */
const START_FILTERS: [string, Target][] = [
	["apps:", { plugin: "programs" }],
	["settings:", { plugin: "settings" }],
	["documents:", { plugin: "explorer", mode: "kind", kind: "documents" }],
	["folders:", { plugin: "explorer", mode: "kind", kind: "folders" }],
	["photos:", { plugin: "explorer", mode: "kind", kind: "photos" }],
	["videos:", { plugin: "explorer", mode: "kind", kind: "videos" }],
	["music:", { plugin: "explorer", mode: "kind", kind: "music" }],
	["web:", { plugin: "web", anyEngine: true }]
];

export function keywordTargets(config: LauncherConfig): Map<string, Target> {
	const map = new Map<string, Target>();
	const add = (keyword: string, target: Target) => {
		const k = keyword.trim();
		if (k && k !== "*" && !map.has(k)) map.set(k, target);
	};
	for (const id of Object.keys(config.plugins) as PluginId[]) {
		const p = config.plugins[id];
		if (p.enabled && id !== "web") add(p.keyword, { plugin: id });
	}
	if (config.plugins.explorer.enabled && config.explorer.contentEnabled) {
		add(config.explorer.contentKeyword, { plugin: "explorer", mode: "content" });
	}
	if (config.plugins.web.enabled) {
		for (const s of allWebSources(config)) if (s.enabled) add(s.keyword, { plugin: "web" });
	}
	for (const [keyword, target] of START_FILTERS) {
		if (config.plugins[target.plugin].enabled) add(keyword, target);
	}
	return map;
}

/**
 * Flow Launcher's rule: the first word is a keyword when one matches
 * ("kill chrome", "g cats", or just "kill"). Symbol keywords also work
 * without a space (">ipconfig", "doc:report").
 */
export function parseQuery(raw: string, targets: Map<string, Target>): ParsedQuery {
	const text = raw.replace(/^\s+/, "");
	const space = text.search(/\s/);
	const first = space < 0 ? text : text.slice(0, space);
	if (targets.has(first)) {
		return {
			raw,
			keyword: first,
			search: space < 0 ? "" : text.slice(space + 1).replace(/^\s+/, "")
		};
	}
	for (const keyword of targets.keys()) {
		if (
			!/[a-z0-9]/i.test(keyword.slice(-1)) &&
			text.startsWith(keyword) &&
			text.length > keyword.length
		) {
			return { raw, keyword, search: text.slice(keyword.length).replace(/^\s+/, "") };
		}
	}
	return { raw, keyword: null, search: text };
}

/** Expands custom shortcuts ("gh" → "github ") and {clipboard}. */
export function expandShortcuts(
	raw: string,
	config: LauncherConfig,
	clipboard: string | null
): string {
	let text = raw;
	const space = text.search(/\s/);
	const first = space < 0 ? text : text.slice(0, space);
	const shortcut = config.shortcuts.find((s) => s.key && s.key === first);
	if (shortcut) text = shortcut.value + (space < 0 ? "" : text.slice(space));
	if (clipboard !== null) text = text.replace(/\{clipboard\}/gi, clipboard);
	return text;
}

export interface SearchRun {
	/** Results available right away. */
	now: Result[];
	/**
	 * Results that take a moment (files, bookmarks, suggestions, processes).
	 * Started by the caller after a short pause in typing.
	 */
	later: (() => Promise<Result[]>)[];
}

const isGlobal = (config: LauncherConfig, id: PluginId) =>
	config.plugins[id].enabled && config.plugins[id].keyword === "*";

function runGlobal(ctx: SearchContext, search: string, isCurrent: () => boolean): SearchRun {
	const { config } = ctx;
	const now: Result[] = [];
	const later: (() => Promise<Result[]>)[] = [];
	const q = search.trim();
	if (!q) return { now, later };

	const location = isLocationPath(q);
	if (isGlobal(config, "calculator")) now.push(...searchCalculator(ctx, q));
	if (isGlobal(config, "url") && !location) now.push(...searchUrl(ctx, q));
	if (isGlobal(config, "programs") && !location) now.push(...searchPrograms(ctx, q));
	if (isGlobal(config, "system") && !location) now.push(...searchSystem(ctx, q));
	if (isGlobal(config, "settings") && !location) now.push(...searchWindowsSettings(ctx, q));
	if (isGlobal(config, "shell")) now.push(...searchShell(ctx, q).slice(0, 1));
	if (config.plugins.web.enabled && !location) {
		now.push(...searchWeb(ctx, null, q));
		later.push(() => webSuggestions(ctx, null, q, isCurrent));
	}
	if (isGlobal(config, "explorer")) {
		if (location) {
			if (config.explorer.pathSearch) later.push(() => searchPath(ctx, q));
		} else {
			now.push(...searchQuickAccess(ctx, q));
			later.push(() => searchIndex(ctx, q, "name", isCurrent));
		}
	}
	if (isGlobal(config, "bookmarks") && !location && q.length > 1) {
		later.push(() => searchBookmarks(ctx, q, isCurrent));
	}
	if (isGlobal(config, "commands") && !location) now.push(...searchCommands(ctx, q));
	if (isGlobal(config, "windows") && !location) {
		later.push(() => searchWindows(ctx, q, isCurrent));
	}
	return { now, later };
}

function runKeyword(
	ctx: SearchContext,
	target: Target,
	keyword: string,
	search: string,
	isCurrent: () => boolean
): SearchRun {
	const now: Result[] = [];
	const later: (() => Promise<Result[]>)[] = [];
	switch (target.plugin) {
		case "programs":
			now.push(...searchPrograms(ctx, search));
			break;
		case "explorer":
			if (target.mode === "content")
				later.push(() => searchIndex(ctx, search, "content", isCurrent));
			else if (target.mode === "kind")
				later.push(() => searchIndex(ctx, search, "name", isCurrent, target.kind));
			else if (isLocationPath(search)) later.push(() => searchPath(ctx, search));
			else {
				now.push(...searchQuickAccess(ctx, search));
				later.push(() => searchIndex(ctx, search, "name", isCurrent));
			}
			break;
		case "calculator":
			now.push(...searchCalculator(ctx, search));
			break;
		case "url":
			now.push(...searchUrl(ctx, search));
			break;
		case "settings":
			now.push(...searchWindowsSettings(ctx, search));
			break;
		case "system":
			now.push(...searchSystem(ctx, search));
			break;
		case "shell":
			now.push(...searchShell(ctx, search));
			break;
		case "processes":
			later.push(() => searchProcesses(ctx, search, isCurrent));
			break;
		case "bookmarks":
			later.push(() => searchBookmarks(ctx, search, isCurrent));
			break;
		case "indicator":
			now.push(...searchIndicator(ctx, search));
			break;
		case "windows":
			later.push(() => searchWindows(ctx, search, isCurrent, true));
			break;
		case "commands":
			now.push(...searchCommands(ctx, search));
			break;
		case "web": {
			// "web:" uses the engines offered under every search.
			const engine = target.anyEngine ? null : keyword;
			now.push(...searchWeb(ctx, engine, search));
			later.push(() => webSuggestions(ctx, engine, search, isCurrent));
			break;
		}
	}
	return { now, later };
}

export function runSearch(
	ctx: SearchContext,
	parsed: ParsedQuery,
	targets: Map<string, Target>,
	isCurrent: () => boolean
): SearchRun & { keywordEmpty: boolean } {
	if (parsed.keyword) {
		const target = targets.get(parsed.keyword)!;
		const run = runKeyword(ctx, target, parsed.keyword, parsed.search, isCurrent);
		return { ...run, keywordEmpty: !parsed.search && !/\s/.test(parsed.raw) };
	}
	return { ...runGlobal(ctx, parsed.search, isCurrent), keywordEmpty: false };
}

/** A typed keyword with nothing after it can still mean a normal search. */
export function runFallback(
	ctx: SearchContext,
	parsed: ParsedQuery,
	isCurrent: () => boolean
): SearchRun {
	return runGlobal(ctx, parsed.raw.trim(), isCurrent);
}

export function sortResults(results: Result[]): Result[] {
	const seen = new Set<string>();
	return results
		.map((r, i) => ({ r, i }))
		.sort((a, b) => b.r.score - a.r.score || a.i - b.i)
		.map((x) => x.r)
		.filter((r) => {
			if (seen.has(r.key)) return false;
			seen.add(r.key);
			return true;
		});
}

export function makeContext(
	base: Omit<SearchContext, "min" | "boost">,
	boost: (key: string) => number
): SearchContext {
	return { ...base, min: minScore(base.config.precision), boost };
}

function replay(ctx: SearchContext, r: Replay) {
	switch (r.type) {
		case "app":
			return openApp(r.path);
		case "path":
		case "url":
			return open(r.type === "url" ? r.url : r.path);
		case "command-line":
			return runCommandLine(r.command);
		case "system":
			return SYSTEM_COMMANDS.find((c) => c.id === r.id)?.run(ctx);
		case "shell":
			return runShell(ctx, r.command, false);
	}
}

const REPLAY_PLUGIN: Record<Replay["type"], PluginId> = {
	app: "programs",
	path: "explorer",
	url: "web",
	"command-line": "settings",
	system: "system",
	shell: "shell"
};

export function historyResults(ctx: SearchContext, entries: HistoryEntry[], filter = ""): Result[] {
	const q = filter.trim();
	return entries
		.map((e, i) => ({ e, score: q ? scoreMatch(q, e.title) : 1000 - i }))
		.filter((x) => !q || x.score >= ctx.min)
		.map(({ e, score }) => ({
			key: e.key,
			title: e.title,
			subtitle: e.subtitle,
			icon: e.icon,
			score,
			plugin: REPLAY_PLUGIN[e.replay.type],
			run: () => replay(ctx, e.replay),
			replay: e.replay,
			actions: () => [
				{ key: "open", title: "Open", glyph: "external", run: () => replay(ctx, e.replay) },
				{
					key: "forget",
					title: "Remove from history",
					glyph: "trash",
					keepOpen: true,
					run: () => {
						forgetHistory(e.key);
						ctx.requery();
					}
				}
			]
		}));
}

/** The home page: recently opened results, when that's turned on. */
export function homeResults(ctx: SearchContext): Result[] {
	if (!ctx.config.homeHistory) return [];
	return historyResults(ctx, recentHistory(ctx.config.homeHistoryCount));
}
