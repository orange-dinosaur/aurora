import { describe, expect, it } from "vitest";
import { differences } from "../differences";
import { blocks, tally } from "./compare";

const plain = { strong: false, em: false, difference: null };

describe("blocks", () => {
	it("draws emphasis without its markers and washes a changed word", () => {
		expect(blocks("Hello *big* world", [{ from: 7, to: 10 }])).toEqual([
			{
				kind: "paragraph",
				runs: [
					{ ...plain, text: "Hello " },
					{ text: "big", strong: false, em: true, difference: 0 },
					{ ...plain, text: " world" },
				],
			},
		]);
	});

	it("marks where words are missing with an empty run", () => {
		expect(blocks("One two.", [{ from: 3, to: 3 }])).toEqual([
			{
				kind: "paragraph",
				runs: [
					{ ...plain, text: "One" },
					{ ...plain, text: "", difference: 0 },
					{ ...plain, text: " two." },
				],
			},
		]);
	});

	it("breaks the body into the blocks the editor draws", () => {
		const text = "# Title\n\nFirst\nline\n\n> quoted\n\n* * *\n\n- item";
		expect(blocks(text, [])).toEqual([
			{ kind: "heading", level: 1, runs: [{ ...plain, text: "Title" }] },
			{
				kind: "paragraph",
				runs: [
					{ ...plain, text: "First" },
					{ ...plain, text: "\n" },
					{ ...plain, text: "line" },
				],
			},
			{ kind: "quote", runs: [{ ...plain, text: "quoted" }] },
			{ kind: "break" },
			{ kind: "item", runs: [{ ...plain, text: "item" }] },
		]);
	});

	it("leaves the front matter out and keeps offsets into the whole text", () => {
		const text = "---\ntitle: x\n---\nBody";
		const from = text.indexOf("Body");
		expect(blocks(text, [{ from, to: from + 4 }])).toEqual([
			{
				kind: "paragraph",
				runs: [{ ...plain, text: "Body", difference: 0 }],
			},
		]);
	});
});

describe("tally", () => {
	it("counts the words each side has that the other does not", () => {
		const then = "a b c";
		const now = "a x y c";
		expect(tally(then, now, differences(then, now))).toEqual({
			added: 2,
			removed: 1,
		});
	});
});
