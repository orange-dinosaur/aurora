import { describe, expect, it } from "vitest";
import { differences } from "../differences";
import { blocks, change, follow, tally } from "./compare";

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

describe("change", () => {
	it("tells words put in, taken out and swapped apart", () => {
		const found = differences("a b c", "a x c d").concat(
			differences("a b", "a"),
		);
		expect(found.map(change)).toEqual(["swapped", "added", "removed"]);
	});
});

describe("follow", () => {
	const pairs: [number, number][] = [
		[100, 100],
		[300, 150],
		[500, 350],
	];

	it("moves in step where both sides match", () => {
		expect(follow(pairs, 50)).toBe(50);
		expect(follow(pairs, 400)).toBe(250);
	});

	it("moves in proportion across a difference", () => {
		expect(follow(pairs, 200)).toBe(125);
	});

	it("carries on in step past the last pair", () => {
		expect(follow(pairs, 600)).toBe(450);
	});

	it("skips a pair that runs backwards", () => {
		const crossed: [number, number][] = [
			[100, 100],
			[90, 200],
			[300, 300],
		];
		expect(follow(crossed, 200)).toBe(200);
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
