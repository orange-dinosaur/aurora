import { describe, expect, test } from "vitest";
import {
	face,
	FACES,
	moved,
	nudged,
	SETTINGS,
	type Setting,
} from "./typography";
import type { Preferences } from "../types";

function find(id: Setting["id"]): Setting {
	const setting = SETTINGS.find((each) => each.id === id);
	if (setting === undefined) {
		throw new Error(`no setting ${id}`);
	}
	return setting;
}

const DEFAULTS: Preferences = {
	toolbar: true,
	focus: false,
	typewriter: false,
	sidebar: true,
	rightSidebar: false,
	rightSidebarTab: "synopsis",
	sidebarWidth: 248,
	rightSidebarWidth: 300,
	theme: "system",
	manuscriptFont: "newsreader",
	defaultSprint: null,
	defaultSprintUnit: "words",
	defaultTarget: null,
	idleMinutes: 30,
	sessionClock: true,
	measure: 68,
	fontSize: 16,
	lineHeight: 1.7,
};

describe("nudging a setting", () => {
	test("one press moves it one step", () => {
		expect(moved(find("fontSize"), 16, 1)).toBe(17);
		expect(moved(find("measure"), 68, -1)).toBe(66);
	});

	// 1.7 + 0.1 is 1.7999999999999998, which would reach both the store and
	// the writer's eyes.
	test("a tenth does not drift", () => {
		expect(moved(find("lineHeight"), 1.7, 1)).toBe(1.8);
		expect(moved(find("lineHeight"), 1.9, -1)).toBe(1.8);
	});

	test("it stops at the ends of its range", () => {
		const spacing = find("lineHeight");
		expect(moved(spacing, spacing.max, 1)).toBe(spacing.max);
		expect(moved(spacing, spacing.min, -1)).toBe(spacing.min);
	});

	test("stepping the whole range never leaves it", () => {
		for (const setting of SETTINGS) {
			let value = setting.min;
			for (let press = 0; press < 100; press += 1) {
				value = moved(setting, value, 1);
				expect(value).toBeLessThanOrEqual(setting.max);
				expect(value).toBeGreaterThanOrEqual(setting.min);
			}
			expect(value).toBe(setting.max);
		}
	});

	test("only the setting pressed changes", () => {
		expect(nudged(DEFAULTS, find("fontSize"), 1)).toEqual({
			...DEFAULTS,
			fontSize: 17,
		});
	});
});

describe("what the popover shows", () => {
	test.each([
		["measure", 68, "68"],
		["fontSize", 16, "16px"],
		["lineHeight", 1.7, "1.7"],
		["lineHeight", 2, "2.0"],
	])("%s at %s reads as %s", (id, value, expected) => {
		expect(find(id as Setting["id"]).show(value)).toBe(expected);
	});
});

describe("the manuscript faces", () => {
	test("every face the picker offers names a family first", () => {
		for (const each of FACES) {
			expect(face(each.id).length).toBeGreaterThan(0);
		}
	});

	test("each stack ends in a generic family, so something is always drawn", () => {
		for (const each of FACES) {
			expect(face(each.id)).toMatch(/(serif|sans-serif|monospace)$/);
		}
	});

	test("no face is offered twice", () => {
		expect(new Set(FACES.map((each) => each.id)).size).toBe(FACES.length);
	});
});
