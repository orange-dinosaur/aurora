import { describe, expect, test } from "vitest";
import { flipped, showing } from "./theme";

describe("showing", () => {
	test("an explicit theme is what is drawn, whatever the desktop says", () => {
		expect(showing("light", true)).toBe("light");
		expect(showing("dark", false)).toBe("dark");
	});

	test("following the desktop takes the desktop's answer", () => {
		expect(showing("system", true)).toBe("dark");
		expect(showing("system", false)).toBe("light");
	});
});

describe("flipped", () => {
	test("it moves away from whatever is drawn", () => {
		expect(flipped("light", false)).toBe("dark");
		expect(flipped("dark", false)).toBe("light");
	});

	// The trap the three states set: on a dark desktop, System is already dark,
	// so a button that only knew the preference would switch to dark and
	// appear to do nothing.
	test("following a dark desktop switches to light, not to dark", () => {
		expect(flipped("system", true)).toBe("light");
	});

	test("following a light desktop switches to dark", () => {
		expect(flipped("system", false)).toBe("dark");
	});
});
