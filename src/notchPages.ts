/**
 * The notch's pages that scrolling, swiping and the arrows go through, in
 * the order chosen in Settings > Notch > Pages (`bloom-notch-page-order`).
 * Each page is switched on or off by its own setting, as before; the
 * controls page is always on, as the notch's fallback.
 */

export type NotchPageId = "calendar" | "command-center" | "shelf" | "notes" | "blitzit";

export const PAGE_ORDER_KEY = "bloom-notch-page-order";

export const DEFAULT_PAGE_ORDER: readonly NotchPageId[] = [
	"calendar",
	"command-center",
	"shelf",
	"notes",
	"blitzit"
];

export const PAGE_LABELS: Record<NotchPageId, string> = {
	calendar: "Calendar & Timer",
	"command-center": "Controls",
	shelf: "Shelf",
	notes: "Notes & To-dos",
	blitzit: "Blitzit"
};

/** The setting that switches each page on or off (none: always on). */
export const PAGE_FLAG_KEYS: Record<NotchPageId, string | null> = {
	calendar: "bloom-calendar-enabled",
	"command-center": null,
	shelf: "bloom-notch-shelf-in-scroll",
	notes: "bloom-notch-notes-in-scroll",
	blitzit: "bloom-notch-blitzit"
};

export const isNotchPageId = (value: unknown): value is NotchPageId =>
	DEFAULT_PAGE_ORDER.includes(value as NotchPageId);

/**
 * The saved order: every page once, in the saved order, with pages it
 * doesn't name (added in a later Bloom) at the end.
 */
export function parsePageOrder(raw: unknown): NotchPageId[] {
	let saved: unknown = raw;
	if (typeof raw === "string") {
		try {
			saved = JSON.parse(raw);
		} catch {
			saved = null;
		}
	}
	const order: NotchPageId[] = [];
	if (Array.isArray(saved)) {
		for (const id of saved) {
			if (isNotchPageId(id) && !order.includes(id)) order.push(id);
		}
	}
	for (const id of DEFAULT_PAGE_ORDER) {
		if (!order.includes(id)) order.push(id);
	}
	return order;
}

/** A page moved one place up (-1) or down (1). */
export function movePage(order: NotchPageId[], id: NotchPageId, by: -1 | 1): NotchPageId[] {
	const from = order.indexOf(id);
	const to = from + by;
	if (from < 0 || to < 0 || to >= order.length) return order;
	const next = [...order];
	[next[from], next[to]] = [next[to], next[from]];
	return next;
}
