import { createLucideIcon } from "lucide-react";

/**
 * G-Helper's mark (the "G" in a circle it shows in the tray), drawn as a
 * line icon like the rest of Bloom's settings, so it follows the theme.
 */
export const GHelperIcon = createLucideIcon("GHelper", [
	["circle", { cx: "12", cy: "12", r: "10", key: "ring" }],
	["path", { d: "M15.8 8.8A5 5 0 1 0 17 12H12.5", key: "g" }]
]);
