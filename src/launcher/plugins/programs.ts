import type { AppInfo } from "../../dockApps";
import { bestMatch } from "../fuzzy";
import { copy, open, openApp, reveal, resolveAppPath } from "../ops";
import { saveConfig } from "../config";
import type { Result, ResultAction, SearchContext } from "../types";

/** Packaged (Store) apps are launched by id, not by a file. */
function isPackaged(path: string) {
	return !/[\\/]/.test(path) || path.toLowerCase().startsWith("shell:appsfolder");
}

function appActions(
	ctx: SearchContext,
	app: { name: string; path: string }
): () => Promise<ResultAction[]> {
	return async () => {
		const file = isPackaged(app.path) ? null : await resolveAppPath(app.path).catch(() => null);
		const actions: ResultAction[] = [
			{ key: "open", title: "Open", glyph: "external", run: () => openApp(app.path) }
		];
		if (!isPackaged(app.path)) {
			actions.push({
				key: "admin",
				title: "Run as administrator",
				subtitle: "Ctrl+Shift+Enter",
				glyph: "admin",
				run: () => open(file ?? app.path, "runas")
			});
		}
		if (file) {
			actions.push({
				key: "location",
				title: "Open file location",
				subtitle: "Ctrl+Enter",
				glyph: "folder",
				run: () => reveal(file)
			});
			actions.push({
				key: "copy-path",
				title: "Copy path",
				glyph: "copy",
				run: () => copy(file)
			});
		}
		actions.push({
			key: "hide",
			title: "Hide from results",
			subtitle: "Bring it back under Settings > Search Bar Pill > Programs",
			glyph: "ban",
			keepOpen: true,
			run: () => {
				const config = ctx.config;
				if (!config.programs.hidden.includes(app.path)) {
					config.programs.hidden = [...config.programs.hidden, app.path];
					saveConfig(config);
				}
				ctx.requery();
			}
		});
		return actions;
	};
}

export function appResult(
	ctx: SearchContext,
	app: { name: string; path: string },
	score: number,
	subtitle?: string
): Result {
	const key = `app:${app.path.toLowerCase()}`;
	const packaged = isPackaged(app.path);
	return {
		key,
		title: app.name,
		subtitle: subtitle ?? (ctx.config.programs.showPath && !packaged ? app.path : "Application"),
		icon: { kind: "app", path: app.path, name: app.name },
		score: score + ctx.boost(key),
		plugin: "programs",
		run: () => openApp(app.path),
		runAsAdmin: packaged
			? undefined
			: async () => {
					const file = await resolveAppPath(app.path).catch(() => null);
					return open(file ?? app.path, "runas");
				},
		openLocation: packaged ? undefined : () => revealApp(app.path),
		actions: appActions(ctx, app),
		copyText: packaged ? app.name : app.path,
		preview: { kind: "app", path: app.path },
		replay: { type: "app", path: app.path, name: app.name }
	};
}

/** Installed apps (and, when turned on, programs found on PATH). */
export function searchPrograms(ctx: SearchContext, search: string): Result[] {
	const q = search.trim();
	if (!q) return [];
	const hidden = new Set(ctx.config.programs.hidden);
	const seen = new Set<string>();
	const out: Result[] = [];

	const consider = (
		app: AppInfo | { name: string; path: string; executable?: string },
		subtitle?: string
	) => {
		if (hidden.has(app.path)) return;
		const exe = app.executable?.replace(/\.exe$/i, "");
		const score = bestMatch(q, [app.name, exe], [1, 0.8]);
		if (score < ctx.min) return;
		const id = app.name.toLowerCase();
		if (seen.has(id)) return;
		seen.add(id);
		// Programs outrank files, settings and web searches for the same match.
		out.push(appResult(ctx, app, score * 0.6 + 400, subtitle));
	};

	for (const app of ctx.apps) consider(app);
	if (ctx.config.programs.pathSource) {
		for (const entry of ctx.pathPrograms) {
			consider(
				{ name: entry.name.replace(/\.exe$/i, ""), path: entry.path, executable: entry.name },
				entry.path
			);
		}
	}
	return out;
}

/** "Open file location" for Ctrl+Enter on an app: resolved lazily. */
export async function revealApp(path: string) {
	const file = await resolveAppPath(path).catch(() => null);
	if (file) return reveal(file);
}
