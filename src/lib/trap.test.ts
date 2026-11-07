import { describe, expect, test } from "vitest";
import { wrapTo } from "./trap";

describe("wrapTo", () => {
	test("Tab from the last control goes to the first", () => {
		expect(wrapTo(4, 3, false)).toBe(0);
	});

	test("Shift+Tab from the first control goes to the last", () => {
		expect(wrapTo(4, 0, true)).toBe(3);
	});

	test("leaves the middle to the browser", () => {
		expect(wrapTo(4, 1, false)).toBeNull();
		expect(wrapTo(4, 2, true)).toBeNull();
	});

	test("Tab from the dialog itself goes to the first control", () => {
		expect(wrapTo(4, -1, false)).toBe(0);
	});

	test("Shift+Tab from the dialog itself goes to the last control", () => {
		expect(wrapTo(4, -1, true)).toBe(3);
	});

	test("does nothing with no controls", () => {
		expect(wrapTo(0, -1, false)).toBeNull();
	});
});
