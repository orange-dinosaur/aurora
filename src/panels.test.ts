import { describe, expect, test } from "vitest";
import { dragged, RIGHT_SIDEBAR, SIDEBAR } from "./panels";

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
