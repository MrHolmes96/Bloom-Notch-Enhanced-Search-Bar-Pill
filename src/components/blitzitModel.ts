/**
 * Blitzit's lists, tasks and columns as the notch shows them (shared by the
 * list view, `BlitzitPanel`, and the board, `BlitzitBoard`).
 */

export interface List {
	id: string;
	name: string;
	/** Its colour in Blitzit, when Blitzit says. */
	color?: string;
	/** Its open tasks, as far as this PC knows. */
	open?: number;
}

export interface Task {
	id: string;
	name: string;
	done: boolean;
	column: string;
	/** The list it's in. */
	list_id: string;
	/** Not in Blitzit yet. */
	pending: boolean;
}

export interface Board {
	connected: boolean;
	lists: List[];
	list: List | null;
	tasks: Task[];
	columns: string[];
	can_add: boolean;
	can_complete: boolean;
	can_move: boolean;
	offline: boolean;
	problem: string | null;
	pending: number;
	synced_at: number | null;
}

export interface Column {
	/** What Blitzit calls it ("THIS_WEEK"); "" for tasks with no column. */
	key: string;
	label: string;
	done: boolean;
}

/** Every list's tasks together, as Blitzit's own "All Lists". */
export const ALL_LISTS = "__all__";
export const DONE_KEY = "__done";

export function squash(text: string) {
	return text.toLowerCase().replace(/[^a-z0-9]/g, "");
}

/** "THIS_WEEK" → "This Week", as Blitzit writes its columns. */
export function pretty(name: string) {
	const words = name.replace(/[_-]+/g, " ").trim();
	if (!words) return name;
	return words
		.split(/\s+/)
		.map((w) => (w === w.toUpperCase() ? w.charAt(0) + w.slice(1).toLowerCase() : w))
		.map((w) => w.charAt(0).toUpperCase() + w.slice(1))
		.join(" ");
}

/** Blitzit's own order: Backlog, This Week, Today, then anything else. */
function rank(key: string) {
	const k = squash(key);
	if (k.includes("backlog") || k.includes("someday") || k.includes("later")) return 0;
	if (k.includes("week")) return 1;
	if (k.includes("today")) return 2;
	return 1.5;
}

export function isDoneName(key: string) {
	return ["done", "completed", "complete", "finished"].includes(squash(key));
}

/** The list's columns: the ones Blitzit names and the ones its tasks are in (and Done). */
export function columnsOf(board: Board | null, withDone = true): Column[] {
	if (!board) return [];
	const found: Column[] = [];
	const add = (key: string) => {
		if (isDoneName(key) || found.some((c) => squash(c.key) === squash(key))) return;
		found.push({ key, label: pretty(key), done: false });
	};
	board.columns.forEach(add);
	board.tasks.filter((t) => t.column).forEach((t) => add(t.column));
	found.sort((a, b) => rank(a.key) - rank(b.key));
	// Tasks in no column Blitzit names (or no columns at all) still get one.
	if (!found.length || board.tasks.some((t) => !t.done && !columnFor(found, t.column))) {
		found.push({ key: "", label: found.length ? "No Column" : "Tasks", done: false });
	}
	if (withDone) found.push({ key: DONE_KEY, label: "Done", done: true });
	return found;
}

export function columnFor(columns: Column[], name: string) {
	if (!name) return columns.find((c) => c.key === "" && !c.done);
	return columns.find((c) => !c.done && squash(c.key) === squash(name));
}

export function inColumn(task: Task, column: Column, columns: Column[]) {
	if (column.done) return task.done;
	if (task.done) return false;
	return columnFor(columns, task.column) === column;
}

export function isToday(column: Column) {
	return squash(column.key).includes("today");
}

export function syncedText(board: Board | null) {
	if (!board) return "";
	if (board.offline) {
		return board.pending > 0
			? `Offline · ${board.pending} change${board.pending === 1 ? "" : "s"} will sync when you're back online`
			: "Offline · showing your tasks as last synced";
	}
	if (board.pending > 0) return `Syncing ${board.pending} change${board.pending === 1 ? "" : "s"}…`;
	if (!board.synced_at) return "Not synced yet";
	const time = new Date(board.synced_at).toLocaleTimeString([], {
		hour: "2-digit",
		minute: "2-digit",
		hour12: false
	});
	return `Synced with Blitzit at ${time}`;
}

/** A list's colour: Blitzit's when it says, else Blitzit's default green. */
export function listColor(list: List | null | undefined) {
	const c = list?.color?.trim();
	if (!c) return "#4caf50";
	return /^[0-9a-f]{3}([0-9a-f]{3})?$/i.test(c) ? `#${c}` : c;
}

export function listLetter(list: List | null | undefined) {
	return (list?.name?.trim().charAt(0) || "B").toUpperCase();
}
