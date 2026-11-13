import { describe, expect, test } from "vitest";
import {
	divider,
	dragged,
	RIGHT_SIDEBAR,
	share,
	SIDE_MIN,
	SIDEBAR,
} from "./panels";

describe("the divider of a split", () => {
	test("leaves each side at least the minimum", () => {
		expect(divider(1000)).toEqual({ min: SIDE_MIN, max: 1000 - SIDE_MIN });
	});

	test("never asks for a range that runs backwards", () => {
		const tight = divider(500);
		expect(tight.max).toBeGreaterThanOrEqual(tight.min);
	});

	test("is kept as the left side's share in whole percent", () => {
		expect(share(350, 700)).toBe(50);
		expect(share(455, 700)).toBe(65);
	});

	test("falls back to halves before the page has been measured", () => {
		expect(share(300, 0)).toBe(50);
	});
});

describe("dragging a panel edge", () => {
	test("the left panel follows the pointer", () => {
		expect(dragged("left", 248, 40, SIDEBAR)).toBe(288);
		expect(dragged("left", 248, -40, SIDEBAR)).toBe(208);
	});

	test("the right panel goes the other way", () => {
		expect(dragged("right", 300, 40, RIGHT_SIDEBAR)).toBe(260);
		expect(dragged("right", 300, -40, RIGHT_SIDEBAR)).toBe(340);
	});

	test("neither goes past its bounds", () => {
		expect(dragged("left", 248, -900, SIDEBAR)).toBe(SIDEBAR.min);
		expect(dragged("left", 248, 900, SIDEBAR)).toBe(SIDEBAR.max);
		expect(dragged("right", 300, 900, RIGHT_SIDEBAR)).toBe(
			RIGHT_SIDEBAR.min,
		);
		expect(dragged("right", 300, -900, RIGHT_SIDEBAR)).toBe(
			RIGHT_SIDEBAR.max,
		);
	});

	// The width the drag started at is the one that matters. Measuring from
	// the width now would leave a pointer that ran past the end of the range
	// and came back dragging from somewhere it never was.
	test("a drag out of range and back lands where the pointer is", () => {
		expect(dragged("left", 248, -900, SIDEBAR)).toBe(SIDEBAR.min);
		expect(dragged("left", 248, -20, SIDEBAR)).toBe(228);
	});
});
