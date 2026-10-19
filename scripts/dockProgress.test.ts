import { describe, expect, test } from "bun:test";
import type { AppInfo } from "../src/dockApps";
import { pickProgress, progressByItem, type WindowProgress } from "../src/dockProgress";

const app = (extra: Partial<AppInfo>): AppInfo => ({
	name: "App",
	path: "C:\\App\\app.exe",
	icon: null,
	is_running: true,
	...extra
});

const progress = (extra: Partial<WindowProgress>): WindowProgress => ({
	hwnd: 1,
	owner: 1,
	programs: ["C:\\App\\app.exe"],
	windows: [],
	state: "normal",
	value: 0.5,
	...extra
});

const key = (a: AppInfo) => a.name;

describe("dock progress", () => {
	test("goes to the item holding the window", () => {
		const items = [app({ name: "Steam", hwnd: 10, all_hwnds: [[10, "Steam"]] }), app({ name: "Brave", hwnd: 20 })];
		const shown = progressByItem(items, items, [progress({ hwnd: 10, owner: 10, programs: ["C:\\Steam\\steam.exe"] })], key);
		expect(shown.get("Steam")).toEqual({ state: "normal", value: 0.5 });
		expect(shown.has("Brave")).toBe(false);
	});

	test("a dialog's progress goes to its main window's item", () => {
		const items = [app({ name: "Files", hwnd: 30 })];
		const shown = progressByItem(items, items, [progress({ hwnd: 31, owner: 30 })], key);
		expect(shown.get("Files")?.value).toBe(0.5);
	});

	test("an app in the tray gets it by program", () => {
		const pinned = app({ name: "Steam", path: "C:\\Users\\me\\Steam.lnk", hwnd: 40, is_background: true });
		const running = [app({ name: "Steam", path: "C:\\Steam\\steam.exe", hwnd: 40 })];
		const shown = progressByItem([pinned], running, [progress({ hwnd: 99, owner: 99, programs: ["C:\\Steam\\steam.exe"] })], key);
		expect(shown.get("Steam")?.value).toBe(0.5);
	});

	test("Steam's helper window goes to Steam, minimized or in the tray", () => {
		const steam = app({ name: "Steam", path: "C:\\Steam\\steam.exe", hwnd: 60, is_background: true });
		const helper = progress({
			hwnd: 61,
			owner: 61,
			programs: ["C:\\Steam\\bin\\cef\\cef.win64\\steamwebhelper.exe", "C:\\Steam\\steam.exe"]
		});
		expect(progressByItem([steam], [steam], [helper], key).get("Steam")?.value).toBe(0.5);
	});

	test("a hidden window's progress goes to the item holding the app's other windows", () => {
		const items = [
			app({ name: "Steam", path: "C:\\Steam\\bin\\cef\\steamwebhelper.exe", hwnd: 70 }),
			app({ name: "Game", path: "D:\\Games\\game.exe", hwnd: 80 })
		];
		const hidden = progress({ hwnd: 71, owner: 71, programs: ["C:\\Steam\\steam.exe"], windows: [71, 70, 72] });
		expect(progressByItem(items, items, [hidden], key).get("Steam")?.value).toBe(0.5);
		expect(progressByItem(items, items, [hidden], key).has("Game")).toBe(false);
	});

	test("two items of one program (web apps) get none of an unknown window's", () => {
		const items = [
			app({ name: "Edge", path: "C:\\Edge\\msedge.exe", hwnd: 50 }),
			app({ name: "Outlook", path: "C:\\Edge\\msedge.exe", hwnd: 51 })
		];
		const shown = progressByItem(items, items, [progress({ hwnd: 52, owner: 52, programs: ["C:\\Edge\\msedge.exe"] })], key);
		expect(shown.size).toBe(0);
	});

	test("Windows' order: error, paused, the furthest-behind bar, indeterminate", () => {
		expect(pickProgress([progress({ value: 0.9 }), progress({ value: 0.2 })])).toEqual({ state: "normal", value: 0.2 });
		expect(pickProgress([progress({}), progress({ state: "paused", value: null })])).toEqual({ state: "paused", value: 1 });
		expect(pickProgress([progress({ state: "indeterminate", value: null }), progress({ value: 0.3 })])?.value).toBe(0.3);
		expect(pickProgress([progress({ state: "indeterminate", value: 0.4 })])).toEqual({ state: "indeterminate", value: null });
		expect(pickProgress([progress({ value: null })])).toEqual({ state: "normal", value: 0 });
	});
});
