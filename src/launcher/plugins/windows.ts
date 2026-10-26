import { invoke } from "@tauri-apps/api/core";
import type { AppInfo } from "../../dockApps";
import { bestMatch } from "../fuzzy";
import type { Result, SearchContext } from "../types";

/**
 * Open Windows: every window that is open, found by its title or its app's
 * name, as Start's search and Alt+Tab find them. Enter switches to it.
 */

async function openWindows(): Promise<AppInfo[]> {
	return invoke<AppInfo[]>("get_active_windows").catch(() => []);
}

export async function searchWindows(
	ctx: SearchContext,
	search: string,
	isCurrent: () => boolean,
	/** Through the plugin's keyword: every window, also with nothing typed. */
	listAll = false
): Promise<Result[]> {
	const q = search.trim();
	if (!q && !listAll) return [];
	const apps = await openWindows();
	if (!isCurrent()) return [];
	const out: Result[] = [];
	for (const app of apps) {
		const windows = app.all_hwnds?.length
			? app.all_hwnds
			: app.hwnd
				? ([[app.hwnd, app.name]] as [number, string][])
				: [];
		for (const [hwnd, title] of windows) {
			if (!hwnd) continue;
			const label = title?.trim() || app.name;
			// Below an app of the same name: opening it is what most mean.
			const score = q ? bestMatch(q, [label, app.name], [0.9, 0.75]) : 500;
			if (q && score < ctx.min) continue;
			out.push({
				key: `window:${hwnd}`,
				title: label,
				subtitle: label === app.name ? "Open window" : `Open window · ${app.name}`,
				icon: { kind: "app", path: app.path, name: app.name },
				score,
				plugin: "windows",
				noHistory: true,
				run: () => invoke("switch_to_window", { hwnd })
			});
		}
	}
	return out;
}
