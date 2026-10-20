import { describe, expect, test } from "bun:test";
import { DEFAULT_PAGE_ORDER, movePage, parsePageOrder } from "../src/notchPages";

describe("notch page order", () => {
	test("nothing saved: the default order", () => {
		expect(parsePageOrder(null)).toEqual([...DEFAULT_PAGE_ORDER]);
		expect(parsePageOrder("not json")).toEqual([...DEFAULT_PAGE_ORDER]);
	});

	test("the saved order, each page once, unknown ones dropped", () => {
		const saved = JSON.stringify(["shelf", "notes", "shelf", "music", "calendar", "command-center", "blitzit"]);
		expect(parsePageOrder(saved)).toEqual(["shelf", "notes", "calendar", "command-center", "blitzit"]);
	});

	test("a page missing from an older saved order goes at the end", () => {
		expect(parsePageOrder(["notes", "calendar", "command-center", "shelf"])).toEqual([
			"notes",
			"calendar",
			"command-center",
			"shelf",
			"blitzit"
		]);
		expect(parsePageOrder(["shelf", "command-center"])).toEqual([
			"shelf",
			"command-center",
			"calendar",
			"notes",
			"blitzit"
		]);
	});

	test("moving a page swaps it with its neighbour, and stops at the ends", () => {
		const order = [...DEFAULT_PAGE_ORDER];
		expect(movePage(order, "shelf", -1)).toEqual(["calendar", "shelf", "command-center", "notes", "blitzit"]);
		expect(movePage(order, "calendar", -1)).toEqual(order);
		expect(movePage(order, "blitzit", 1)).toEqual(order);
	});
});
