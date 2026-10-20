import { describe, expect, test } from "bun:test";
import { isoWeek } from "../src/weekNumber";

describe("ISO week number", () => {
	test("ordinary weeks", () => {
		expect(isoWeek(new Date(2026, 9, 20))).toBe(43);
		expect(isoWeek(new Date(2026, 9, 19))).toBe(43); // Monday
		expect(isoWeek(new Date(2026, 9, 18))).toBe(42); // Sunday ends the week
	});

	test("weeks across the new year", () => {
		expect(isoWeek(new Date(2026, 0, 1))).toBe(1); // a Thursday
		expect(isoWeek(new Date(2027, 0, 1))).toBe(53); // a Friday: still 2026's last week
		expect(isoWeek(new Date(2024, 11, 30))).toBe(1); // a Monday: already 2025's first week
		expect(isoWeek(new Date(2021, 0, 3))).toBe(53);
	});
});
