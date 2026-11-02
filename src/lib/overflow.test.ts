import { describe, expect, test } from "vitest";
import { outOfView } from "./overflow";

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
