import { describe, expect, test } from "vitest";
import { matches, type Run } from "./find";

/** One paragraph, split into as many runs as it is given. */
function block(key: string, ...texts: string[]): Run[] {
	return texts.map((text, at) => ({ key: `${key}.${at}`, text, block: key }));
}

describe("finding text", () => {
	test("an empty query finds nothing", () => {
		expect(matches(block("p1", "Sing to me of the man"), "")).toEqual([]);
	});

	test("a match reports the run it is in and where", () => {
		const found = matches(block("p1", "Sing to me, Muse"), "me");

		expect(found).toEqual([
			{ fromKey: "p1.0", fromOffset: 8, toKey: "p1.0", toOffset: 10 },
		]);
	});

	test("case is ignored", () => {
		expect(matches(block("p1", "Sing to me, Muse"), "muse")).toHaveLength(
			1,
		);
	});

	test("a match can run from one piece of a paragraph into the next", () => {
		// What a bold word in mid-sentence looks like to the editor.
		const found = matches(block("p1", "the ", "Mu", "se sang"), "muse");

		expect(found).toEqual([
			{ fromKey: "p1.1", fromOffset: 0, toKey: "p1.2", toOffset: 2 },
		]);
	});

	test("a match never crosses from one block into the next", () => {
		const runs = [...block("p1", "of the"), ...block("p2", "man")];

		expect(matches(runs, "of theman")).toEqual([]);
		expect(matches(runs, "man")).toHaveLength(1);
	});

	test("matches do not overlap each other", () => {
		expect(matches(block("p1", "aaaa"), "aa")).toHaveLength(2);
	});

	test("empty runs are passed over", () => {
		const found = matches(block("p1", "", "Muse", ""), "Muse");

		expect(found).toEqual([
			{ fromKey: "p1.1", fromOffset: 0, toKey: "p1.1", toOffset: 4 },
		]);
	});

	// "İ".toLowerCase() is two characters, which would put every offset after
	// it out by one. The search turns exact rather than turning wrong.
	test("a letter that grows when lowercased does not shift the offsets", () => {
		const found = matches(block("p1", "İstanbul, and Muse"), "Muse");

		expect(found).toEqual([
			{ fromKey: "p1.0", fromOffset: 14, toKey: "p1.0", toOffset: 18 },
		]);
	});
});
