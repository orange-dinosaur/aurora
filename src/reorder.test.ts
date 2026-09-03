import { describe, expect, test } from "vitest";
import { accepts, index } from "./reorder";
import type { Spot } from "./reorder";
import { MANUSCRIPT } from "./kinds";

/** A scene at the top of the Manuscript, which is what the rest differ from. */
const ROW: Spot = {
	id: "row",
	at: 0,
	group: "manuscript",
	section: MANUSCRIPT,
	kind: null,
	folder: false,
	holds: 0,
	within: false,
};

function spot(what: Partial<Spot>): Spot {
	return { ...ROW, ...what };
}

const part = spot({ id: "part", at: 0, kind: "part", folder: true, holds: 2 });
const chapter = spot({
	id: "chapter",
	at: 1,
	kind: "chapter",
	folder: true,
	holds: 3,
});

describe("what a row will take", () => {
	test("a scene goes inside a chapter it is aimed at", () => {
		const scene = spot({ id: "scene", at: 0, group: "part" });
		expect(accepts(scene, chapter, "into")).toBe(true);
	});

	test("a chapter goes inside a part", () => {
		expect(accepts(chapter, part, "into")).toBe(true);
	});

	test("a chapter will not hold a part", () => {
		expect(accepts(part, chapter, "into")).toBe(false);
	});

	test("a document holds nothing, whatever it is aimed at", () => {
		const scene = spot({ id: "scene", at: 2, group: "chapter" });
		expect(accepts(chapter, scene, "into")).toBe(false);
	});

	// Outside the Manuscript a folder holds any other folder, so this is the
	// one place a drag could be aimed at somewhere it already covers.
	test("nothing lands in a row drawn inside what is being dragged", () => {
		const research = spot({
			id: "research",
			section: "Notes",
			group: "notes",
			folder: true,
			holds: 2,
		});
		const sources = spot({
			id: "sources",
			at: 1,
			section: "Notes",
			group: "research",
			folder: true,
		});
		expect(accepts(research, sources, "into")).toBe(true);
		expect(accepts(research, { ...sources, within: true }, "into")).toBe(
			false,
		);
	});

	test("a node does not land in the folder it already sits in", () => {
		const scene = spot({ id: "scene", at: 0, group: "chapter" });
		expect(accepts(scene, chapter, "into")).toBe(false);
	});

	test("a row lands beside only what it sits with", () => {
		const scene = spot({ id: "scene", at: 0, group: "chapter" });
		expect(accepts(scene, part, "before")).toBe(false);
	});

	test("a drop where it already is is not a move", () => {
		const scene = spot({ id: "scene", at: 1 });
		const above = spot({ id: "above", at: 0 });
		const below = spot({ id: "below", at: 2 });
		expect(accepts(scene, above, "after")).toBe(false);
		expect(accepts(scene, below, "before")).toBe(false);
		expect(accepts(scene, above, "before")).toBe(true);
	});
});

describe("the index a drop asks for", () => {
	test("inside a folder is the end of what it holds", () => {
		const scene = spot({ id: "scene", at: 0, group: "part" });
		expect(index(scene, chapter, "into")).toBe(3);
	});

	test("moving down, the row it lands on has come up one", () => {
		const scene = spot({ id: "scene", at: 0 });
		const target = spot({ id: "target", at: 3 });
		expect(index(scene, target, "before")).toBe(2);
		expect(index(scene, target, "after")).toBe(3);
	});

	test("moving up, the row it lands on has not moved", () => {
		const scene = spot({ id: "scene", at: 3 });
		const target = spot({ id: "target", at: 1 });
		expect(index(scene, target, "before")).toBe(1);
		expect(index(scene, target, "after")).toBe(2);
	});

	test("landing in another folder, nothing has been lifted out of it", () => {
		const scene = spot({ id: "scene", at: 0, group: "chapter" });
		const target = spot({ id: "target", at: 2, group: "part" });
		expect(index(scene, target, "before")).toBe(2);
		expect(index(scene, target, "after")).toBe(3);
	});
});
