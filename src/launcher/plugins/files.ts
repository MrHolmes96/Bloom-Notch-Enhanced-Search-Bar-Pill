import { invoke } from "@tauri-apps/api/core";
import { bestMatch, scoreMatch } from "../fuzzy";
import { saveConfig } from "../config";
import {
	baseName,
	copy,
	copyFiles,
	open,
	openTerminal,
	parentFolder,
	recycle,
	reveal,
	systemCommand
} from "../ops";
import type { DirEntry, Result, ResultAction, SearchContext } from "../types";

interface IndexHit {
	path: string;
	name: string;
	is_dir: boolean;
}

/** "C:\...", "\\server\share", "%APPDATA%\..." or "~\...". */
export function isLocationPath(text: string) {
	return /^([a-zA-Z]:[\\/]|\\\\[^\\]|%[^%\s]+%|~([\\/]|$))/.test(text.trim());
}

function expandHome(text: string) {
	return text.replace(/^~(?=[\\/]|$)/, "%USERPROFILE%");
}

function excludedType(ctx: SearchContext, path: string) {
	const types = ctx.config.explorer.excludedTypes
		.split(/[,;\s]+/)
		.map((t) =>
			t
				.trim()
				.replace(/^\*?\./, "")
				.toLowerCase()
		)
		.filter(Boolean);
	if (!types.length) return false;
	const ext = path.split(".").pop()?.toLowerCase() ?? "";
	return types.includes(ext);
}

function fileActions(ctx: SearchContext, path: string, isDir: boolean): () => ResultAction[] {
	return () => {
		const { explorer } = ctx.config;
		const inQuickAccess = explorer.quickAccess.some((p) => p.toLowerCase() === path.toLowerCase());
		const lower = path.toLowerCase();
		const runnable = /\.(exe|bat|cmd|msi|ps1|com)$/.test(lower);
		const actions: ResultAction[] = [
			{
				key: "open",
				title: isDir ? "Open folder" : "Open",
				glyph: "external",
				run: () => open(path)
			},
			{
				key: "location",
				title: "Open containing folder",
				subtitle: "Ctrl+Enter",
				glyph: "folder",
				run: () => reveal(path)
			}
		];
		if (!isDir) {
			actions.push({
				key: "open-with",
				title: "Open with…",
				glyph: "open-with",
				run: () => open(path, "openas")
			});
		}
		if (runnable) {
			actions.push({
				key: "admin",
				title: "Run as administrator",
				subtitle: "Ctrl+Shift+Enter",
				glyph: "admin",
				run: () => open(path, "runas")
			});
		}
		actions.push({
			key: "terminal",
			title: "Open in terminal",
			glyph: "terminal",
			run: () => openTerminal(isDir ? path : parentFolder(path), explorer.terminal)
		});
		actions.push(
			{ key: "copy-path", title: "Copy path", glyph: "copy", run: () => copy(path) },
			{ key: "copy-name", title: "Copy name", glyph: "copy", run: () => copy(baseName(path)) },
			{
				key: "copy-file",
				title: isDir ? "Copy folder" : "Copy file",
				subtitle: "Paste it in Explorer",
				glyph: "copy",
				run: () => copyFiles([path])
			},
			{
				key: "quick-access",
				title: inQuickAccess ? "Remove from Quick Access" : "Add to Quick Access",
				subtitle: "Quick Access items show first in search",
				glyph: "star",
				keepOpen: true,
				run: () => {
					explorer.quickAccess = inQuickAccess
						? explorer.quickAccess.filter((p) => p.toLowerCase() !== path.toLowerCase())
						: [...explorer.quickAccess, path];
					saveConfig(ctx.config);
					ctx.notify(inQuickAccess ? "Removed from Quick Access" : "Added to Quick Access");
				}
			},
			{
				key: "exclude",
				title: "Exclude from search",
				subtitle: isDir ? "This folder and everything in it" : "This file",
				glyph: "ban",
				keepOpen: true,
				run: () => {
					if (!explorer.excluded.includes(path)) {
						explorer.excluded = [...explorer.excluded, path];
						saveConfig(ctx.config);
					}
					ctx.requery();
				}
			},
			{
				key: "recycle",
				title: "Move to Recycle Bin",
				glyph: "trash",
				confirm: `Press Enter again to move ${baseName(path)} to the Recycle Bin`,
				keepOpen: true,
				run: async () => {
					try {
						await recycle(path);
						ctx.notify(`Moved ${baseName(path)} to the Recycle Bin`);
						ctx.requery();
					} catch (e) {
						ctx.notify(String(e));
					}
				}
			},
			{
				key: "properties",
				title: "Properties",
				glyph: "properties",
				run: () => open(path, "properties")
			},
			{
				key: "indexing",
				title: "Open Windows Indexing Options",
				subtitle: "Choose which folders Windows Search covers",
				glyph: "index",
				run: () => systemCommand("indexing-options")
			}
		);
		return actions;
	};
}

export function fileResult(
	ctx: SearchContext,
	path: string,
	isDir: boolean,
	score: number,
	name?: string
): Result {
	const key = `path:${path.toLowerCase()}`;
	const runnable = /\.(exe|bat|cmd|msi|ps1|com)$/i.test(path);
	return {
		key,
		title: name ?? baseName(path),
		subtitle: path,
		icon: { kind: "file", path },
		score: score + ctx.boost(key),
		plugin: "explorer",
		run: () => open(path),
		openLocation: () => reveal(path),
		runAsAdmin: runnable ? () => open(path, "runas") : undefined,
		autoComplete: isDir ? `${path.replace(/[\\/]+$/, "")}\\` : path,
		copyText: path,
		actions: fileActions(ctx, path, isDir),
		preview: { kind: isDir ? "folder" : "file", path },
		replay: { type: "path", path, isDir }
	};
}

/** Quick Access links whose names match. */
export function searchQuickAccess(ctx: SearchContext, search: string): Result[] {
	const q = search.trim();
	if (!q) return [];
	return ctx.config.explorer.quickAccess
		.map((path) => ({ path, score: scoreMatch(q, baseName(path)) }))
		.filter((x) => x.score >= ctx.min)
		.map((x) => fileResult(ctx, x.path, !/\.[a-z0-9]{1,6}$/i.test(x.path), 380 + x.score * 0.5));
}

/** Browsing a typed path: "C:\Users\me\Doc" lists C:\Users\me matching "Doc". */
export async function searchPath(ctx: SearchContext, text: string): Promise<Result[]> {
	const typed = expandHome(text.trim());
	const endsWithSep = /[\\/]$/.test(typed);
	const folder = endsWithSep ? typed : parentFolder(typed);
	const partial = endsWithSep ? "" : baseName(typed);
	let entries: DirEntry[] = [];
	try {
		entries = await invoke<DirEntry[]>("launcher_list_dir", { path: folder, max: 400 });
	} catch {
		return [];
	}
	const out: Result[] = [];
	if (endsWithSep || /^[a-zA-Z]:$/.test(typed)) {
		out.push(fileResult(ctx, folder, true, 1300, `Open ${folder}`));
	}
	for (const entry of entries) {
		if (excludedType(ctx, entry.path)) continue;
		const score = partial ? scoreMatch(partial, entry.name) : 500;
		if (partial && score < ctx.min) continue;
		out.push(
			fileResult(ctx, entry.path, entry.is_dir, 900 + score * 0.3 + (entry.is_dir ? 20 : 0))
		);
	}
	return out;
}

let warnedOff = false;

/** Files and folders from the Windows Search index, by name or by content. */
export async function searchIndex(
	ctx: SearchContext,
	search: string,
	mode: "name" | "content",
	isCurrent: () => boolean,
	/** Start's file filters: "documents", "folders", "photos", "videos", "music". */
	kind?: string
): Promise<Result[]> {
	const q = search.trim();
	if (q.length < 2) return [];
	let hits: IndexHit[];
	try {
		hits = await invoke<IndexHit[]>("launcher_search_index", {
			query: q,
			mode,
			max: ctx.config.explorer.maxResults,
			excluded: ctx.config.explorer.excluded,
			kind: kind ?? null
		});
	} catch (e) {
		const message = String(e);
		if (!isCurrent() || warnedOff) return [];
		if (/turned off/i.test(message)) {
			warnedOff = true;
			return [
				{
					key: "index-off",
					title: "Windows Search is turned off",
					subtitle: "File search needs the Windows Search service. Open Indexing Options",
					icon: { kind: "glyph", glyph: "warning" },
					score: 10,
					plugin: "explorer",
					noHistory: true,
					run: () => systemCommand("indexing-options")
				}
			];
		}
		return [];
	}
	if (!isCurrent()) return [];
	return hits
		.filter((hit) => !excludedType(ctx, hit.path))
		.map((hit, rank) => {
			const nameScore = mode === "name" ? bestMatch(q, [hit.name]) : 0;
			return fileResult(ctx, hit.path, hit.is_dir, 250 + nameScore * 0.3 - rank * 2, hit.name);
		});
}
