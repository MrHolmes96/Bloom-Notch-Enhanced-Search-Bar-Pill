/**
 * Which tray icons show in the notch's tray. Every live tray icon shows,
 * except the hidden ones: hidden in Bloom, or hidden by Tray Icon Manager
 * while Bloom follows it. Hidden icons are one click away in the tray's
 * "Hidden icons" menu. What is changed in Bloom is kept in
 * `bloom-tray-placement` and only affects Bloom; Windows' tray and Tray Icon
 * Manager's settings are never changed.
 */

export type Visibility = "shown" | "hidden";

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
	/** On Windows' taskbar rather than in its overflow. */
	promoted: boolean;
	/** Tray Icon Manager hides it. */
	manager_hidden: boolean;
	/** Hidden from Windows' own tray (by Tray Icon Manager, or by the app). */
	hidden_in_windows?: boolean;
	/** The icon's live picture, as Windows' tray shows it right now. */
	image?: string | null;
}

export interface TrayState {
	icons: TrayIconInfo[];
	manager_found: boolean;
	manager_path: string | null;
	manager_entries: { name: string; key: string | null; running: boolean }[];
}

/** Bloom's own choices. Older builds stored "tray" / "menu" / "hidden". */
export function parsePlacements(raw: string | null): Record<string, Visibility> {
	try {
		const parsed: unknown = JSON.parse(raw ?? "{}");
		if (!parsed || typeof parsed !== "object") return {};
		const result: Record<string, Visibility> = {};
		for (const [key, value] of Object.entries(parsed as Record<string, unknown>)) {
			if (value === "shown" || value === "tray") result[key] = "shown";
			else if (value === "hidden") result[key] = "hidden";
		}
		return result;
	} catch {
		return {};
	}
}

/**
 * Before anything is chosen in Bloom: hidden when Tray Icon Manager hides it
 * (while Bloom follows it), or when it is hidden from Windows' own tray for
 * another reason, so the notch's tray matches Windows' and every running icon
 * is there, the hidden ones under "Hidden icons".
 */
export function defaultVisibility(icon: TrayIconInfo, followManager: boolean): Visibility {
	if (icon.manager_hidden) return followManager ? "hidden" : "shown";
	return icon.hidden_in_windows ? "hidden" : "shown";
}

export function visibilityOf(
	icon: TrayIconInfo,
	placements: Record<string, Visibility>,
	followManager: boolean
): Visibility {
	return placements[icon.key] ?? defaultVisibility(icon, followManager);
}

/**
 * Shows or hides an icon. Choosing what Windows and Tray Icon Manager already
 * say removes Bloom's own choice, so later changes there carry through.
 */
export function withVisibility(
	placements: Record<string, Visibility>,
	icon: TrayIconInfo,
	visibility: Visibility,
	followManager: boolean
): Record<string, Visibility> {
	const next = { ...placements };
	if (visibility === defaultVisibility(icon, followManager)) delete next[icon.key];
	else next[icon.key] = visibility;
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
