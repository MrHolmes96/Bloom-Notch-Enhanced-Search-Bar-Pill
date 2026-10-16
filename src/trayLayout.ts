/**
 * Where each tray icon shows in the notch's tray: in the tray itself, in the
 * hidden icons below it, or not at all. Windows' own placement (taskbar or
 * hidden icons) and Tray Icon Manager's hidden list are the starting point;
 * what is changed in Bloom is kept in `bloom-tray-placement` and only affects
 * Bloom. Windows' tray and Tray Icon Manager's settings are never changed.
 */

export type Placement = "tray" | "menu" | "hidden";

export const TRAY_PLACEMENT_KEY = "bloom-tray-placement";
export const TRAY_FOLLOW_KEY = "bloom-tray-follow-manager";

export interface TrayIconInfo {
	/** Windows' id for the icon, to click it. */
	id: string;
	/** Bloom's lasting name for it. */
	key: string;
	name: string;
	tooltip: string;
	path: string;
	/** On Windows' taskbar rather than in its hidden icons. */
	promoted: boolean;
	/** Tray Icon Manager hides it. */
	manager_hidden: boolean;
}

export interface TrayState {
	icons: TrayIconInfo[];
	manager_found: boolean;
	manager_entries: { name: string; key: string | null }[];
}

const PLACEMENTS: Placement[] = ["tray", "menu", "hidden"];

export function parsePlacements(raw: string | null): Record<string, Placement> {
	try {
		const parsed: unknown = JSON.parse(raw ?? "{}");
		if (!parsed || typeof parsed !== "object") return {};
		const result: Record<string, Placement> = {};
		for (const [key, value] of Object.entries(parsed as Record<string, unknown>)) {
			if (PLACEMENTS.includes(value as Placement)) result[key] = value as Placement;
		}
		return result;
	} catch {
		return {};
	}
}

/** Where Windows and Tray Icon Manager put it, before anything set in Bloom. */
export function defaultPlacement(icon: TrayIconInfo, followManager: boolean): Placement {
	if (followManager && icon.manager_hidden) return "hidden";
	return icon.promoted ? "tray" : "menu";
}

export function placementOf(
	icon: TrayIconInfo,
	placements: Record<string, Placement>,
	followManager: boolean
): Placement {
	return placements[icon.key] ?? defaultPlacement(icon, followManager);
}

/**
 * Sets an icon's place. Choosing the default removes Bloom's own choice, so
 * a later change in Windows or Tray Icon Manager carries through again.
 */
export function withPlacement(
	placements: Record<string, Placement>,
	icon: TrayIconInfo,
	place: Placement,
	followManager: boolean
): Record<string, Placement> {
	const next = { ...placements };
	if (place === defaultPlacement(icon, followManager)) delete next[icon.key];
	else next[icon.key] = place;
	return next;
}

/** The tile's label: the app's name, or its icon's tooltip when one app has several. */
export function iconLabels(icons: TrayIconInfo[]): Map<string, string> {
	const counts = new Map<string, number>();
	for (const icon of icons) counts.set(icon.name, (counts.get(icon.name) ?? 0) + 1);
	const labels = new Map<string, string>();
	const used = new Map<string, number>();
	for (const icon of icons) {
		if ((counts.get(icon.name) ?? 1) === 1) {
			labels.set(icon.id, icon.name);
			continue;
		}
		const n = (used.get(icon.name) ?? 0) + 1;
		used.set(icon.name, n);
		labels.set(icon.id, icon.tooltip || `${icon.name} ${n}`);
	}
	return labels;
}
