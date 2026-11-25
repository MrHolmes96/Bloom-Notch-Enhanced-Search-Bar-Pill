/**
 * Widgets in the strips beside the dock: what is placed where, and the grid
 * they sit on (see DockWidgets.tsx).
 *
 * Each side is a row of grid cells the dock's resting height, counted from
 * the screen's edge inward (cell 0 is the one at the edge), so a widget stays
 * exactly where it was placed whatever the dock does: grows with apps,
 * stretches to a full taskbar, shrinks, bounces. Where the dock reaches into
 * a widget's cells, that widget steps aside until there's room again. An item
 * takes 1, 2 or 4 cells (small, wide, large) and holds one widget, or several
 * as a group that the mouse wheel flips through. Kept in settings.json as
 * `bloom-dock-widgets` (version 2; version 1 counted from the dock outward).
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
export type DesktopRule = "smart" | "show" | "hide";
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
	/** The first cell it takes, counted from the screen's edge inward. */
	cell: number;
	size: WidgetSize;
	/** One widget, or a group the mouse wheel flips through. */
	widgets: WidgetKind[];
	/** A group's own turn: seconds between widgets, 0 for none. */
	rotate: number;
	/**
	 * When it steps aside (for a group, the whole group): "smart" (the
	 * default) hides it in the calm moments (see `smartHide`) and while the
	 * notch on screen shows the same, and pointing at its spot brings it
	 * back; "show" always shows it; "hide" hides it while the desktop is in
	 * front, and pointing doesn't bring it back.
	 */
	desktop?: DesktopRule;
	/**
	 * Smart's calm moments: "desktop" only while the desktop is in front;
	 * absent, also while the notch is on screen (a small window in front).
	 */
	smartHide?: "desktop";
	/** Per-widget choices (a world clock's time zone, a countdown's date). */
	options?: Partial<Record<WidgetKind, Record<string, string>>>;
}

export interface Layout {
	/** 2: cells from the screen's edge; 1 (older): from the dock outward. */
	version: 1 | 2;
	left: Placed[];
	right: Placed[];
}

export const EMPTY: Layout = { version: 2, left: [], right: [] };
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
						desktop: item?.desktop === "show" || item?.desktop === "hide" ? item.desktop : undefined,
						smartHide: item?.smartHide === "desktop" ? "desktop" : undefined,
						options: typeof item?.options === "object" && item.options ? item.options : undefined
					};
				})
				.filter((p): p is Placed => p !== null);
		const version = raw?.version === 1 ? 1 : 2;
		return { version, left: settle(side(raw?.left)), right: settle(side(raw?.right)) };
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
	/** The shelf's inner padding (as the dock's), and its flares' width. */
	pad: number;
	feet: number;
	/** How many cells fit on each side, edge to middle. */
	cells: Record<Side, number>;
	/** Where cell 0 is: on the left its left edge, on the right its right edge. */
	origin: Record<Side, number>;
}

/**
 * The grid for a screen `width` wide and a dock `height` tall at rest: cells
 * from each screen edge in to the middle. Fixed: nothing the dock does moves it.
 */
export function gridFor(height: number, width: number, scale: number): Grid {
	const pad = 8 * scale;
	const gap = 8 * scale;
	const feet = 13 * scale;
	const cell = Math.max(24, height - pad * 2);
	const pitch = cell + gap;
	const leftOrigin = feet + pad;
	const rightOrigin = width - feet - pad;
	const room = (space: number) => Math.max(0, Math.floor((space + gap) / pitch));
	const half = width / 2;
	return {
		cell,
		gap,
		pad,
		feet,
		cells: { left: room(half - leftOrigin), right: room(rightOrigin - half) },
		origin: { left: leftOrigin, right: rightOrigin }
	};
}

/**
 * How many cells from each edge are free of the dock where it is now (its
 * edges and the flares of both it and the shelf, with a pad between).
 */
export function freeCells(grid: Grid, dock: { left: number; right: number }) {
	const pitch = grid.cell + grid.gap;
	const clear = 2 * grid.feet + grid.pad;
	const left = Math.floor((dock.left - clear - grid.origin.left - grid.cell) / pitch) + 1;
	const right = Math.floor((grid.origin.right - grid.cell - (dock.right + clear)) / pitch) + 1;
	return {
		left: Math.max(0, Math.min(grid.cells.left, left)),
		right: Math.max(0, Math.min(grid.cells.right, right))
	};
}

/** Where an item is on the screen: its left edge and width. */
export function placeOf(grid: Grid, side: Side, item: Pick<Placed, "cell" | "size">) {
	const pitch = grid.cell + grid.gap;
	const width = SPAN[item.size] * grid.cell + (SPAN[item.size] - 1) * grid.gap;
	const left =
		side === "left" ? grid.origin.left + item.cell * pitch : grid.origin.right - item.cell * pitch - width;
	return { left, width };
}

/**
 * A version 1 layout (cells from the dock outward, with the dock's edges at
 * `dock`) moved to the fixed grid: each item to the cell nearest where it was.
 */
export function fromVersion1(layout: Layout, grid: Grid, dock: { left: number; right: number }): Layout {
	const pitch = grid.cell + grid.gap;
	const clear = 2 * grid.feet + grid.pad;
	const move = (side: Side, items: Placed[]) =>
		settle(
			items.map((p) => {
				const width = SPAN[p.size] * grid.cell + (SPAN[p.size] - 1) * grid.gap;
				const left = side === "left" ? dock.left - clear - p.cell * pitch - width : dock.right + clear + p.cell * pitch;
				const cell =
					side === "left"
						? Math.round((left - grid.origin.left) / pitch)
						: Math.round((grid.origin.right - width - left) / pitch);
				return { ...p, cell: Math.max(0, cell) };
			})
		);
	return { version: 2, left: move("left", layout.left), right: move("right", layout.right) };
}

/**
 * Whether an item is on screen now: "shown"; "smart", hidden until pointed
 * at; or "gone", hidden. A group is one: hidden only when all of it would be.
 */
export type Visibility = "shown" | "smart" | "gone";

export function visibility(
	item: Placed,
	editing: boolean,
	notch: { shown: boolean; kinds: ReadonlySet<string> },
	onDesktop: boolean
): Visibility {
	const rule = item.desktop ?? "smart";
	if (editing || rule === "show") return "shown";
	if (rule === "hide") return onDesktop ? "gone" : "shown";
	const calm = item.smartHide === "desktop" ? onDesktop : onDesktop || notch.shown;
	const hides = (k: WidgetKind) => calm || (notch.shown && notch.kinds.has(k));
	return item.widgets.every(hides) ? "smart" : "shown";
}
