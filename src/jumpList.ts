import { invoke } from "@tauri-apps/api/core";
import type { AppInfo } from "./dockApps";

export interface JumpItem {
	/** Handle for `open_jump_item`; valid until the next jump list is read. */
	id: number;
	title: string;
	separator: boolean;
}

export interface JumpGroup {
	name: string;
	kind: "pinned" | "recent" | "frequent" | "custom" | "tasks";
	items: JumpItem[];
}

/** True for dock items that are real apps with a jump list to look up. */
export function hasJumpList(app: AppInfo | null | undefined): app is AppInfo {
	return !!app && app.path !== "start" && !app.is_background && !!app.path;
}

/** Reads the app's native jump list (empty when it has none). */
export async function fetchJumpList(app: AppInfo): Promise<JumpGroup[]> {
	const hwnd = app.hwnd ?? app.all_hwnds?.[0]?.[0] ?? null;
	try {
		return await invoke<JumpGroup[]>("get_jump_list", { path: app.path, hwnd });
	} catch {
		return [];
	}
}

export function openJumpItem(id: number) {
	invoke("open_jump_item", { id }).catch(console.error);
}
