import type { Icon, Replay, Result } from "./types";

/**
 * What the search bar remembers, all in Bloom's local storage: how
 * often each result was picked (for ranking), recently opened results (the
 * home page and Ctrl+H), typed queries (Alt+Up/Down) and shell commands.
 */

const USAGE_KEY = "launcher-usage";
const HISTORY_KEY = "launcher-history";
const QUERIES_KEY = "launcher-queries";
const SHELL_KEY = "launcher-shell-history";
const MAX_HISTORY = 50;

export interface HistoryEntry {
	key: string;
	title: string;
	subtitle?: string;
	icon: Icon;
	replay: Replay;
	time: number;
}

function read<T>(key: string, fallback: T): T {
	try {
		const raw = localStorage.getItem(key);
		return raw ? (JSON.parse(raw) as T) : fallback;
	} catch {
		return fallback;
	}
}

function write(key: string, value: unknown) {
	try {
		localStorage.setItem(key, JSON.stringify(value));
	} catch {
		// Storage full or unavailable: ranking just stops learning.
	}
}

let usageCache: Record<string, number> | null = null;

// The settings window can clear the ranking; drop the copy kept here.
window.addEventListener("storage", (e) => {
	if (e.key === null || e.key === USAGE_KEY) usageCache = null;
});

function usage(): Record<string, number> {
	if (!usageCache) usageCache = read<Record<string, number>>(USAGE_KEY, {});
	return usageCache;
}

/** Score boost for a result picked before: up to 300 after 20 picks. */
export function boost(key: string): number {
	return Math.min(usage()[key] ?? 0, 20) * 15;
}

export function recordUse(result: Result, query: string) {
	if (result.noHistory) return;
	const counts = usage();
	counts[result.key] = (counts[result.key] ?? 0) + 1;
	write(USAGE_KEY, counts);

	if (result.replay) {
		const entries = read<HistoryEntry[]>(HISTORY_KEY, []).filter((e) => e.key !== result.key);
		entries.unshift({
			key: result.key,
			title: result.title,
			subtitle: result.subtitle,
			icon: result.icon,
			replay: result.replay,
			time: Date.now()
		});
		write(HISTORY_KEY, entries.slice(0, MAX_HISTORY));
	}

	const q = query.trim();
	if (q) {
		const queries = read<string[]>(QUERIES_KEY, []).filter((x) => x !== q);
		queries.unshift(q);
		write(QUERIES_KEY, queries.slice(0, MAX_HISTORY));
	}
}

export function recentHistory(limit = MAX_HISTORY): HistoryEntry[] {
	return read<HistoryEntry[]>(HISTORY_KEY, []).slice(0, limit);
}

export function forgetHistory(key: string) {
	write(
		HISTORY_KEY,
		read<HistoryEntry[]>(HISTORY_KEY, []).filter((e) => e.key !== key)
	);
}

export function clearHistory() {
	write(HISTORY_KEY, []);
	write(QUERIES_KEY, []);
}

export function queryHistory(): string[] {
	return read<string[]>(QUERIES_KEY, []);
}

/** Shell commands, most used first. */
export function shellHistory(): { command: string; count: number }[] {
	const map = read<Record<string, number>>(SHELL_KEY, {});
	return Object.entries(map)
		.map(([command, count]) => ({ command, count }))
		.sort((a, b) => b.count - a.count);
}

export function recordShell(command: string) {
	const map = read<Record<string, number>>(SHELL_KEY, {});
	map[command] = (map[command] ?? 0) + 1;
	write(SHELL_KEY, map);
}
