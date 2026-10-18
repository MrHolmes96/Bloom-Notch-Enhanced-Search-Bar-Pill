import { describe, expect, test } from "bun:test";
import { defaultVisibility, visibilityOf, type TrayIconInfo } from "../src/trayLayout";

const icon = (extra: Partial<TrayIconInfo>): TrayIconInfo => ({
	id: "g:{1}",
	key: "app.exe",
	name: "App",
	tooltip: "App",
	path: "C:\\App\\app.exe",
	promoted: false,
	manager_hidden: false,
	...extra
});

describe("tray visibility", () => {
	test("a running icon shows", () => {
		expect(defaultVisibility(icon({}), true)).toBe("shown");
	});

	test("Tray Icon Manager's hidden icons are under Hidden while Bloom follows it", () => {
		const hidden = icon({ manager_hidden: true, hidden_in_windows: true });
		expect(defaultVisibility(hidden, true)).toBe("hidden");
		expect(defaultVisibility(hidden, false)).toBe("shown");
	});

	test("an icon hidden from Windows' tray for another reason is under Hidden", () => {
		const hidden = icon({ hidden_in_windows: true });
		expect(defaultVisibility(hidden, true)).toBe("hidden");
		expect(defaultVisibility(hidden, false)).toBe("hidden");
	});

	test("a choice made in Bloom wins", () => {
		const hidden = icon({ manager_hidden: true, hidden_in_windows: true });
		expect(visibilityOf(hidden, { "app.exe": "shown" }, true)).toBe("shown");
	});
});
