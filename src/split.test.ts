import { describe, expect, test } from "vitest";
import {
	act,
	activate,
	close,
	fits,
	focus,
	follow,
	layout,
	openBeside,
	press,
	single,
	swap,
	toggle,
	type Press,
	type Sides,
} from "./split";

// Scene on the left, Notes on the right and current.
const split: Sides = { current: "notes", other: { key: "scene", left: true } };

describe("toggle", () => {
	test("splits with the tab on the left and an empty side to work in", () => {
		expect(toggle(single("scene"))).toEqual({
			current: null,
			other: { key: "scene", left: true },
		});
	});

	test("does nothing with no tab showing", () => {
		expect(toggle(single(null))).toEqual(single(null));
	});

	test("ends a split keeping the current side", () => {
		expect(toggle(split)).toEqual(single("notes"));
	});

	test("ends a split from an empty side keeping the other", () => {
		expect(toggle(toggle(single("scene")))).toEqual(single("scene"));
	});
});

describe("openBeside", () => {
	test("opens a split with the new tab on the right, current", () => {
		expect(openBeside(single("scene"), "notes")).toEqual(split);
	});

	test("on the tab showing, splits as Control+\\ does", () => {
		expect(openBeside(single("scene"), "scene")).toEqual(
			toggle(single("scene")),
		);
	});

	test("with nothing showing, just shows the tab", () => {
		expect(openBeside(single(null), "scene")).toEqual(single("scene"));
	});

	test("replaces the other side's tab and moves there", () => {
		expect(openBeside(split, "mira")).toEqual({
			current: "mira",
			other: { key: "notes", left: false },
		});
	});

	test("on the other side's tab, only moves there", () => {
		expect(openBeside(split, "scene")).toEqual(focus(split, "other"));
	});

	test("fills an empty current side", () => {
		expect(openBeside(toggle(single("scene")), "notes")).toEqual(split);
	});

	test("never puts one tab on both sides", () => {
		for (const sides of [split, single("scene"), toggle(single("scene"))]) {
			for (const key of ["scene", "notes", "mira"]) {
				const after = openBeside(sides, key);
				expect(after.current).not.toBe(after.other?.key);
			}
		}
	});
});

describe("activate", () => {
	test("puts the tab on the current side", () => {
		expect(activate(split, "mira")).toEqual({
			current: "mira",
			other: { key: "scene", left: true },
		});
	});

	test("on the other side's tab, moves there", () => {
		expect(activate(split, "scene")).toEqual(focus(split, "other"));
	});

	test("without a split, works as a tab click always has", () => {
		expect(activate(single("scene"), "notes")).toEqual(single("notes"));
	});
});

describe("follow", () => {
	test("opens a link on the other side when split", () => {
		const search: Sides = {
			current: "search",
			other: { key: "scene", left: true },
		};
		expect(follow(search, "hit")).toEqual({
			current: "hit",
			other: { key: "search", left: false },
		});
	});

	test("opens a link in place without a split", () => {
		expect(follow(single("search"), "hit")).toEqual(single("hit"));
	});
});

describe("focus", () => {
	test("moves to the other side", () => {
		expect(focus(split, "other")).toEqual({
			current: "scene",
			other: { key: "notes", left: false },
		});
	});

	test("moves to the left side by name", () => {
		expect(focus(split, "left")).toEqual(focus(split, "other"));
	});

	test("asking for the side already current does nothing", () => {
		expect(focus(split, "right")).toEqual(split);
	});

	test("does nothing without a split", () => {
		expect(focus(single("scene"), "other")).toEqual(single("scene"));
	});
});

test("swap trades places and keeps the current tab current", () => {
	expect(swap(split)).toEqual({
		current: "notes",
		other: { key: "scene", left: false },
	});
});

describe("close", () => {
	test("closing the current side's tab leaves the other filling the page", () => {
		expect(close(split, "notes")).toEqual(single("scene"));
	});

	test("closing the other side's tab leaves the current filling the page", () => {
		expect(close(split, "scene")).toEqual(single("notes"));
	});

	test("closing a tab on neither side leaves the split", () => {
		expect(close(split, "mira")).toEqual(split);
	});

	test("without a split, leaves the next tab to the caller", () => {
		expect(close(single("scene"), "scene")).toEqual(single("scene"));
	});
});

describe("act", () => {
	test("split and end only go one way", () => {
		expect(act(split, "split")).toEqual(split);
		expect(act(single("scene"), "end")).toEqual(single("scene"));
		expect(act(single("scene"), "split")).toEqual(toggle(single("scene")));
		expect(act(split, "end")).toEqual(single("notes"));
	});

	test("sides go through focus", () => {
		expect(act(split, "left")).toEqual(focus(split, "left"));
	});
});

describe("layout", () => {
	test("places each tab on its side", () => {
		expect(layout(split, true)).toEqual({
			kind: "split",
			left: "scene",
			right: "notes",
		});
	});

	test("folds to the current side without room", () => {
		expect(layout(split, false)).toEqual({
			kind: "folded",
			key: "notes",
			hidden: "scene",
		});
	});

	test("one tab is one tab, room or not", () => {
		expect(layout(single("scene"), false)).toEqual({
			kind: "single",
			key: "scene",
		});
	});

	test("there is room when both sides get the minimum", () => {
		expect(fits(640, 320)).toBe(true);
		expect(fits(639, 320)).toBe(false);
	});
});

describe("press", () => {
	const key = (key: string, held: Partial<Press> = {}): Press => ({
		key,
		ctrlKey: false,
		metaKey: false,
		altKey: false,
		shiftKey: false,
		...held,
	});
	const control = { ctrlKey: true };

	test("Control+\\ toggles", () => {
		expect(press(false, key("\\", control))).toEqual({
			waiting: false,
			action: "toggle",
			swallow: true,
		});
	});

	test("Command+\\ and Command+W are not the split's", () => {
		expect(press(false, key("\\", { metaKey: true })).swallow).toBe(false);
		expect(press(false, key("w", { metaKey: true })).swallow).toBe(false);
	});

	test("Control+W waits for the next key", () => {
		expect(press(false, key("w", control))).toEqual({
			waiting: true,
			action: null,
			swallow: true,
		});
	});

	test.each([
		["h", "left"],
		["l", "right"],
		["w", "other"],
		["v", "split"],
		["q", "end"],
	])("Control+W then %s is %s", (letter, action) => {
		expect(press(true, key(letter))).toEqual({
			waiting: false,
			action,
			swallow: true,
		});
	});

	test("the second key counts with Control still held", () => {
		expect(press(true, key("l", control)).action).toBe("right");
	});

	test("any other second key is let through and ends the wait", () => {
		expect(press(true, key("x"))).toEqual({
			waiting: false,
			action: null,
			swallow: false,
		});
	});

	test("pressing Control again keeps waiting", () => {
		expect(press(true, key("Control", control)).waiting).toBe(true);
	});

	test("ordinary typing is left alone", () => {
		expect(press(false, key("h"))).toEqual({
			waiting: false,
			action: null,
			swallow: false,
		});
	});
});
