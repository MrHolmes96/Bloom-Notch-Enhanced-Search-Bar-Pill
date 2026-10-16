import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSettingsSync } from "../hooks/useSettingsSync";

/** Saves a setting the usual way: local storage, then the backend file. */
export function saveStoredSetting(key: string, value: string) {
	localStorage.setItem(key, value);
	invoke("save_setting", { key, value }).catch(console.error);
}

/** A "true"/"false" setting that also follows changes made elsewhere. */
export function useStoredFlag(key: string, fallback: boolean): [boolean, (next: boolean) => void] {
	const [value, setValue] = useState(() => {
		const stored = localStorage.getItem(key);
		return stored === null ? fallback : stored !== "false";
	});
	useSettingsSync({ [key]: (v: unknown) => setValue(v !== false && v !== "false") });
	const update = (next: boolean) => {
		setValue(next);
		saveStoredSetting(key, String(next));
	};
	return [value, update];
}

/** A text setting that also follows changes made elsewhere. */
export function useStoredString(key: string, fallback: string): [string, (next: string) => void] {
	const [value, setValue] = useState(() => localStorage.getItem(key) ?? fallback);
	useSettingsSync({ [key]: (v: unknown) => setValue(String(v)) });
	const update = (next: string) => {
		setValue(next);
		saveStoredSetting(key, next);
	};
	return [value, update];
}
