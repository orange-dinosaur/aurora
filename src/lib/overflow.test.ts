import { describe, expect, test } from "vitest";
import { inward, outOfView } from "./overflow";

describe("inward", () => {
	const room = { width: 1000, height: 800 };
	const box = { width: 160, height: 100 };

	test("hangs down and right of the point when there is room", () => {
		expect(inward({ x: 300, y: 40 }, box, room)).toEqual({ x: 300, y: 40 });
	});

	test("flips left near the right edge", () => {
		expect(inward({ x: 900, y: 40 }, box, room)).toEqual({ x: 740, y: 40 });
	});

	test("flips up near the bottom", () => {
		expect(inward({ x: 300, y: 750 }, box, room)).toEqual({
			x: 300,
			y: 650,
		});
	});

	test("is pinned to the edge when neither side has room", () => {
		const narrow = { width: 200, height: 800 };
		expect(inward({ x: 100, y: 40 }, box, narrow)).toEqual({
			x: 40,
			y: 40,
		});
	});
});

describe("outOfView", () => {
	test("a row that fits hides nothing", () => {
		expect(outOfView(0, 400, 400)).toEqual({ before: false, after: false });
	});

	test("a fraction of a pixel over still fits", () => {
		expect(outOfView(0, 400.5, 400)).toEqual({
			before: false,
			after: false,
		});
	});

	test("at the start only the far end is hidden", () => {
		expect(outOfView(0, 700, 400)).toEqual({ before: false, after: true });
	});

	test("part way along both ends are hidden", () => {
		expect(outOfView(150, 700, 400)).toEqual({ before: true, after: true });
	});

	test("stopping a hair short of the end counts as the end", () => {
		expect(outOfView(299.5, 700, 400)).toEqual({
			before: true,
			after: false,
		});
	});
});
