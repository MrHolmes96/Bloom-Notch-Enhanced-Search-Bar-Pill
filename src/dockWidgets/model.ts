/**
 * Widgets in the strips beside the dock: what is placed where, and the grid
 * they sit on (see DockWidgets.tsx).
 *
 * Each side is a row of grid cells the dock's height, counted from the dock
 * outward (cell 0 is the one next to the dock), so widgets keep their places
 * next to it as the dock grows or shrinks. An item takes 1, 2 or 4 cells
 * (small, wide, large) and holds one widget, or several as a group that the
 * mouse wheel flips through. Kept in settings.json as `bloom-dock-widgets`.
 */

export type WidgetKind =
	| "weather"
	| "battery"
	| "cpu"
	| "ram"
	| "disk"
	| "net"
	| "clock"
	| "date"
	| "worldclock"
	| "media"
	| "tasks"
	| "focus"
	| "countdown"
	| "progress";

export type WidgetSize = "small" | "wide" | "large";
export type Side = "left" | "right";

export interface WidgetInfo {
	kind: WidgetKind;
	label: string;
	/** What the picker groups it under. */
	section: "Status" | "Clock & calendar" | "Now playing" | "Tasks & timers";
	/** The size it's placed at from the picker. */
	size: WidgetSize;
}

export const WIDGETS: WidgetInfo[] = [
	{ kind: "weather", label: "Weather", section: "Status", size: "small" },
	{ kind: "battery", label: "Battery", section: "Status", size: "small" },
	{ kind: "cpu", label: "CPU", section: "Status", size: "small" },
	{ kind: "ram", label: "RAM", section: "Status", size: "small" },
	{ kind: "disk", label: "Disk", section: "Status", size: "small" },
	{ kind: "net", label: "Network", section: "Status", size: "wide" },
	{ kind: "clock", label: "Clock", section: "Clock & calendar", size: "wide" },
	{ kind: "date", label: "Date", section: "Clock & calendar", size: "small" },
	{ kind: "worldclock", label: "World clock", section: "Clock & calendar", size: "wide" },
	{ kind: "media", label: "Now playing", section: "Now playing", size: "large" },
	{ kind: "tasks", label: "Blitzit today", section: "Tasks & timers", size: "large" },
	{ kind: "focus", label: "Focus timer", section: "Tasks & timers", size: "wide" },
	{ kind: "countdown", label: "Countdown", section: "Tasks & timers", size: "wide" },
	{ kind: "progress", label: "Day & year", section: "Tasks & timers", size: "wide" }
];

export const widgetInfo = (kind: WidgetKind) => WIDGETS.find((w) => w.kind === kind);

/** Cells an item of each size takes. */
export const SPAN: Record<WidgetSize, number> = { small: 1, wide: 2, large: 4 };

export interface Placed {
	id: string;
	/** The first cell it takes, counted from the dock outward. */
	cell: number;
	size: WidgetSize;
	/** One widget, or a group the mouse wheel flips through. */
	widgets: WidgetKind[];
	/** A group's own turn: seconds between widgets, 0 for none. */
	rotate: number;
	/** Per-widget choices (a world clock's time zone, a countdown's date). */
	options?: Partial<Record<WidgetKind, Record<string, string>>>;
}

export interface Layout {
	version: 1;
	left: Placed[];
	right: Placed[];
}

export const EMPTY: Layout = { version: 1, left: [], right: [] };
export const LAYOUT_KEY = "bloom-dock-widgets";

const KINDS = new Set(WIDGETS.map((w) => w.kind));
const SIZES = new Set<WidgetSize>(["small", "wide", "large"]);

/** settings.json's text, or an empty layout for anything unreadable. */
export function readLayout(text: string | null | undefined): Layout {
	if (!text) return EMPTY;
	try {
		const raw = JSON.parse(text);
		const side = (list: unknown): Placed[] =>
			(Array.isArray(list) ? list : [])
				.map((item: any): Placed | null => {
					const widgets = (Array.isArray(item?.widgets) ? item.widgets : []).filter(
						(k: unknown): k is WidgetKind => typeof k === "string" && KINDS.has(k as WidgetKind)
					);
					if (!widgets.length) return null;
					const size: WidgetSize = SIZES.has(item?.size) ? item.size : "small";
					return {
						id: typeof item?.id === "string" ? item.id : newId(),
						cell: Math.max(0, Math.floor(Number(item?.cell) || 0)),
						size,
						widgets,
						rotate: Math.max(0, Math.min(120, Number(item?.rotate) || 0)),
						options: typeof item?.options === "object" && item.options ? item.options : undefined
					};
				})
				.filter((p): p is Placed => p !== null);
		return { version: 1, left: settle(side(raw?.left)), right: settle(side(raw?.right)) };
	} catch {
		return EMPTY;
	}
}

export function newId() {
	return `w${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
}

/** Cells an item takes. */
export const cellsOf = (p: Pick<Placed, "cell" | "size">) =>
	Array.from({ length: SPAN[p.size] }, (_, i) => p.cell + i);

/** Items on a side, overlaps removed (a later one moves to the next free cells). */
export function settle(items: Placed[]): Placed[] {
	const taken = new Set<number>();
	const out: Placed[] = [];
	for (const item of [...items].sort((a, b) => a.cell - b.cell)) {
		let cell = item.cell;
		while (cellsOf({ cell, size: item.size }).some((c) => taken.has(c))) cell += 1;
		cellsOf({ cell, size: item.size }).forEach((c) => taken.add(c));
		out.push({ ...item, cell });
	}
	return out;
}

/** Whether `size` fits at `cell` on a side with `cells` cells, other than `except`. */
export function fits(items: Placed[], cell: number, size: WidgetSize, cells: number, except?: string) {
	if (cell < 0 || cell + SPAN[size] > cells) return false;
	const taken = new Set(items.filter((p) => p.id !== except).flatMap(cellsOf));
	return cellsOf({ cell, size }).every((c) => !taken.has(c));
}

/** The item covering `cell`, if any. */
export const itemAt = (items: Placed[], cell: number) =>
	items.find((p) => cellsOf(p).includes(cell));

/** Runs of neighbouring items: each run is drawn as one shelf. */
export function shelves(items: Placed[]): Placed[][] {
	const sorted = [...items].sort((a, b) => a.cell - b.cell);
	const runs: Placed[][] = [];
	for (const item of sorted) {
		const last = runs[runs.length - 1];
		const prev = last?.[last.length - 1];
		if (prev && prev.cell + SPAN[prev.size] === item.cell) last.push(item);
		else runs.push([item]);
	}
	return runs;
}

/** Grid geometry for one side, in the dock window's CSS pixels. */
export interface Grid {
	/** A cell's size (square) and the space between cells. */
	cell: number;
	gap: number;
	/** The shelf's inner padding (as the dock's). */
	pad: number;
	/** How many cells fit on each side. */
	cells: Record<Side, number>;
	/** Where cell 0 starts: for the left side its right edge, for the right side its left edge. */
	origin: Record<Side, number>;
}

/**
 * The grid beside a dock at `dock` (its left, right and height on screen) in
 * a window `width` wide: cells the dock's inner height, room for the dock's
 * and each shelf's curved feet between them and at the screen's edges.
 */
export function gridFor(
	dock: { left: number; right: number; height: number },
	width: number,
	scale: number
): Grid {
	const pad = 8 * scale;
	const gap = 8 * scale;
	const feet = 13 * scale;
	const cell = Math.max(24, dock.height - pad * 2);
	const pitch = cell + gap;
	const leftOrigin = dock.left - feet * 2 - pad;
	const rightOrigin = dock.right + feet * 2 + pad;
	const room = (space: number) => Math.max(0, Math.floor((space + gap) / pitch));
	return {
		cell,
		gap,
		pad,
		cells: {
			left: room(leftOrigin - feet - pad),
			right: room(width - feet - pad - rightOrigin)
		},
		origin: { left: leftOrigin, right: rightOrigin }
	};
}

/** Where an item sits (left edge and width), in the dock window's CSS pixels. */
export function placeOf(grid: Grid, side: Side, item: Pick<Placed, "cell" | "size">) {
	const pitch = grid.cell + grid.gap;
	const width = SPAN[item.size] * grid.cell + (SPAN[item.size] - 1) * grid.gap;
	const left =
		side === "right"
			? grid.origin.right + item.cell * pitch
			: grid.origin.left - item.cell * pitch - width;
	return { left, width };
}

/** The cell under `x` on a side (may be out of range). */
export function cellAt(grid: Grid, side: Side, x: number) {
	const pitch = grid.cell + grid.gap;
	return side === "right"
		? Math.floor((x - grid.origin.right) / pitch)
		: Math.floor((grid.origin.left - x) / pitch);
}
