/**
 * The round buttons in the notch's control center: Bloom's own six, plus the
 * Windows quick settings that can be added from Settings > Notch.
 */

export type CCButtonId =
	| "dnd"
	| "energy-saver"
	| "tray"
	| "notifications"
	| "bloom-settings"
	| "restart-bloom"
	| "screen-off"
	| "night-light"
	| "airplane"
	| "hotspot"
	| "rotation-lock"
	| "live-captions"
	| "project"
	| "cast"
	| "keyboard-layout"
	| "accessibility"
	| "nearby-sharing"
	| "vpn"
	| "color-filters";

export interface CCButtonInfo {
	id: CCButtonId;
	label: string;
	desc: string;
	/** Handled by the quick_setting_action command. */
	windows?: boolean;
	/** Shows an on/off state, read with quick_settings_state. */
	toggle?: boolean;
}

export const CC_BUTTONS: CCButtonInfo[] = [
	{ id: "dnd", label: "Focus", desc: "Bloom's moon button" },
	{ id: "energy-saver", label: "Energy Saver", desc: "Shows if it's on and opens its settings" },
	{ id: "tray", label: "System Tray", desc: "Your hidden tray icons" },
	{ id: "notifications", label: "Notification Center", desc: "Open Windows' notification center" },
	{ id: "bloom-settings", label: "Bloom Settings", desc: "Open these settings" },
	{ id: "restart-bloom", label: "Restart Bloom", desc: "Restart the notch, dock and search bar" },
	{
		id: "screen-off",
		label: "Screen Off",
		desc: "Turn the screen off; music and videos keep playing (any key wakes it)"
	},
	{
		id: "night-light",
		label: "Night Light",
		desc: "Warmer colors on the screen",
		windows: true,
		toggle: true
	},
	{
		id: "airplane",
		label: "Airplane Mode",
		desc: "Turn off all wireless connections",
		windows: true,
		toggle: true
	},
	{
		id: "hotspot",
		label: "Mobile Hotspot",
		desc: "Share your internet connection",
		windows: true,
		toggle: true
	},
	{
		id: "rotation-lock",
		label: "Rotation Lock",
		desc: "Keep the screen from rotating",
		windows: true,
		toggle: true
	},
	{
		id: "live-captions",
		label: "Live Captions",
		desc: "Captions for any sound on your PC",
		windows: true,
		toggle: true
	},
	{ id: "project", label: "Project", desc: "Choose how a second screen is used", windows: true },
	{ id: "cast", label: "Cast", desc: "Connect to a wireless display", windows: true },
	{
		id: "keyboard-layout",
		label: "Keyboard Layout",
		desc: "Switch to your next keyboard layout",
		windows: true
	},
	{
		id: "accessibility",
		label: "Accessibility",
		desc: "Open accessibility settings",
		windows: true
	},
	{
		id: "nearby-sharing",
		label: "Nearby Sharing",
		desc: "Open nearby sharing settings",
		windows: true
	},
	{ id: "vpn", label: "VPN", desc: "Open VPN settings", windows: true },
	{ id: "color-filters", label: "Color Filters", desc: "Open color filter settings", windows: true }
];

/** The buttons as they were before they could be changed. */
export const CC_DEFAULT: CCButtonId[] = [
	"dnd",
	"energy-saver",
	"tray",
	"notifications",
	"bloom-settings",
	"restart-bloom"
];

export const CC_KEY = "bloom-control-center-buttons";

/** Buttons per row; more wrap onto another row. */
export const CC_PER_ROW = 6;

const KNOWN = new Set<string>(CC_BUTTONS.map((b) => b.id));

/** The shown buttons, in order. "none" means the row is turned off. */
export function parseCCButtons(raw: unknown): CCButtonId[] {
	if (raw === null || raw === undefined || raw === "") return [...CC_DEFAULT];
	const text = String(raw).trim();
	if (text === "none") return [];
	const seen = new Set<string>();
	const ids: CCButtonId[] = [];
	for (const part of text.split(",")) {
		const id = part.trim();
		if (KNOWN.has(id) && !seen.has(id)) {
			seen.add(id);
			ids.push(id as CCButtonId);
		}
	}
	return ids;
}

export function serializeCCButtons(ids: CCButtonId[]): string {
	return ids.length === 0 ? "none" : ids.join(",");
}

export function ccButtonInfo(id: CCButtonId): CCButtonInfo {
	return CC_BUTTONS.find((b) => b.id === id) ?? { id, label: id, desc: "" };
}
