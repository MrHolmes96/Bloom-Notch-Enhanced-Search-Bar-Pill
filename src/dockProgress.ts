import type { AppInfo } from "./dockApps";

export type ProgressState = "normal" | "indeterminate" | "paused" | "error";

/** One window's progress, as its app reported it to the taskbar. */
export interface WindowProgress {
	hwnd: number;
	/** The window that owns it (a dialog's main window), or itself. */
	owner: number;
	/**
	 * Its program, then the programs that started it from the same install:
	 * Steam's window belongs to steamwebhelper.exe, started by steam.exe.
	 */
	programs: string[];
	/** The app's other top-level windows (every process of its install). */
	windows: number[];
	state: ProgressState;
	/** From 0 to 1; null while indeterminate before any value was given. */
	value: number | null;
}

export interface ShownProgress {
	state: ProgressState;
	/** From 0 to 1; null only while indeterminate. */
	value: number | null;
}

/** Windows' order when several windows of one button report progress. */
const PRIORITY: Record<ProgressState, number> = { error: 0, paused: 1, normal: 2, indeterminate: 3 };

const exeName = (path?: string) =>
	(path ?? "").toLowerCase().replace(/\\/g, "/").split("/").pop() ?? "";

const windowsOf = (item: AppInfo) => {
	const all = (item.all_hwnds ?? []).map(([hwnd]) => hwnd);
	if (item.hwnd && !all.includes(item.hwnd)) all.push(item.hwnd);
	return all;
};

/** The one item holding the app's other windows; with several, the one holding most. */
function holderOfWindows(p: WindowProgress, byWindow: Map<number, string>): string | undefined {
	const counts = new Map<string, number>();
	for (const hwnd of p.windows ?? []) {
		const key = byWindow.get(hwnd);
		if (key) counts.set(key, (counts.get(key) ?? 0) + 1);
	}
	let best: string | undefined;
	let most = 0;
	let tie = false;
	for (const [key, count] of counts) {
		if (count > most) {
			best = key;
			most = count;
			tie = false;
		} else if (count === most) {
			tie = true;
		}
	}
	return tie ? undefined : best;
}

/**
 * What one button shows when several of its windows report progress, as on
 * Windows: an error first, then paused, then a normal bar (the one furthest
 * behind), then an indeterminate one.
 */
export function pickProgress(list: WindowProgress[]): ShownProgress | null {
	let best: WindowProgress | null = null;
	for (const p of list) {
		if (!best) {
			best = p;
			continue;
		}
		const a = PRIORITY[p.state];
		const b = PRIORITY[best.state];
		if (a < b || (a === b && (p.value ?? 1) < (best.value ?? 1))) best = p;
	}
	if (!best) return null;
	if (best.state === "indeterminate") return { state: best.state, value: null };
	// Paused or failed before any value: Windows fills the bar; normal starts empty.
	return { state: best.state, value: best.value ?? (best.state === "normal" ? 0 : 1) };
}

/**
 * Each dock item's progress, keyed by `keyOf(item)`. A window's progress goes
 * to the item holding that window (or the window that owns it), else to the
 * item holding the app's other windows (Steam reports on a hidden window while
 * its main one is on the dock). With none on the dock (an app in the tray), it
 * goes to the one item running its program, or else the program that started
 * it (Steam for its helper); when several items run it (web apps of one
 * browser), to none. `running` is the running-apps list, whose paths are the
 * programs themselves.
 */
export function progressByItem(
	items: AppInfo[],
	running: AppInfo[],
	progress: WindowProgress[],
	keyOf: (item: AppInfo) => string
): Map<string, ShownProgress> {
	const shown = new Map<string, ShownProgress>();
	if (progress.length === 0) return shown;

	const programOf = new Map<number, string>();
	for (const app of running) {
		for (const hwnd of windowsOf(app)) programOf.set(hwnd, exeName(app.path));
	}
	const byWindow = new Map<number, string>();
	const programs = new Map<string, Set<string>>();
	for (const item of items) {
		if (!item.is_running) continue;
		const key = keyOf(item);
		const names = new Set<string>([exeName(item.executable), exeName(item.path)]);
		for (const hwnd of windowsOf(item)) {
			byWindow.set(hwnd, key);
			const program = programOf.get(hwnd);
			if (program) names.add(program);
		}
		for (const name of names) {
			if (!name.endsWith(".exe")) continue;
			if (!programs.has(name)) programs.set(name, new Set());
			programs.get(name)!.add(key);
		}
	}

	const found = new Map<string, WindowProgress[]>();
	for (const p of progress) {
		let key = byWindow.get(p.hwnd) ?? byWindow.get(p.owner) ?? holderOfWindows(p, byWindow);
		for (const program of key ? [] : p.programs) {
			const holders = programs.get(exeName(program));
			if (holders?.size === 1) {
				key = [...holders][0];
				break;
			}
			if (holders && holders.size > 1) break;
		}
		if (!key) continue;
		if (!found.has(key)) found.set(key, []);
		found.get(key)!.push(p);
	}
	for (const [key, list] of found) {
		const picked = pickProgress(list);
		if (picked) shown.set(key, picked);
	}
	return shown;
}
