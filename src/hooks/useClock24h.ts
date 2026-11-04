import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSettingsSync } from "./useSettingsSync";

/**
 * Bloom's clock setting (Settings > General > 24-hour clock), for every time
 * Bloom shows: read from settings.json (a window's own copy can be behind)
 * and followed live when it changes.
 */
const KEY = "bloom-time-format-24h";

export function useClock24h(): boolean {
	const [h24, setH24] = useState(() => localStorage.getItem(KEY) === "true");
	useEffect(() => {
		void invoke<Record<string, unknown>>("load_settings")
			.then((settings) => {
				const value = settings?.[KEY];
				if (value === undefined || value === null) return;
				const on = String(value) === "true";
				setH24(on);
				try {
					localStorage.setItem(KEY, String(on));
				} catch {}
			})
			.catch(() => {});
	}, []);
	useSettingsSync({ [KEY]: (value) => setH24(value === true || value === "true") });
	return h24;
}

/** A time of day as the notch's own clock shows it ("13.25" / "01.25 pm"). */
export function formatClock(date: Date, h24: boolean): string {
	return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: !h24 });
}
