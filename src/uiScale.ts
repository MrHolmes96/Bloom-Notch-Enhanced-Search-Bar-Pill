/**
 * The notch and the dock each have their own size (Settings > Appearance >
 * Notch Size / Dock Size). Older versions had one size for everything,
 * "bloom-scale"; a part without its own setting yet keeps using that one.
 * The volume and brightness pop-ups follow the notch.
 */

export type ScalePart = "notch" | "dock";

export const SCALE_KEYS: Record<ScalePart, string> = {
	notch: "bloom-notch-scale",
	dock: "bloom-dock-scale"
};

/** The shared size from older versions. */
export const LEGACY_SCALE_KEY = "bloom-scale";

function parse(value: unknown): number | null {
	const number = typeof value === "number" ? value : parseFloat(String(value ?? ""));
	return Number.isFinite(number) && number > 0 ? number : null;
}

/** A part's size from stored settings, `get` reading one key. */
export function readScale(
	part: ScalePart,
	get: (key: string) => unknown = (key) => localStorage.getItem(key)
): number {
	return parse(get(SCALE_KEYS[part])) ?? parse(get(LEGACY_SCALE_KEY)) ?? 1;
}

/**
 * Settings-sync handlers for a part's size: its own key always applies, the
 * shared one only while the part has no size of its own.
 */
export function scaleSyncHandlers(part: ScalePart, set: (scale: number) => void) {
	return {
		[SCALE_KEYS[part]]: (value: unknown) => {
			const scale = parse(value);
			if (scale !== null) set(scale);
		},
		[LEGACY_SCALE_KEY]: (value: unknown) => {
			if (localStorage.getItem(SCALE_KEYS[part]) !== null) return;
			const scale = parse(value);
			if (scale !== null) set(scale);
		}
	};
}
