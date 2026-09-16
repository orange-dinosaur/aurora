import { describe, expect, test } from "vitest";
import { runsOf } from "./runs";
import { MIN_QUERY, search, type Searchable } from "./search";

/** A document search can read, with its runs taken from real markdown. */
function document(title: string, markdown: string): Searchable {
	return {
		id: `id-${title}`,
		title,
		trail: ["Manuscript"],
		runs: runsOf(markdown),
	};
}

/** A document whose file would not open. */
function unreadable(title: string): Searchable {
	return { id: `id-${title}`, title, trail: ["Manuscript"], runs: null };
}

describe("what comes back", () => {
	test("only documents the query is in", () => {
		const results = search(
			[
				document("Chapter 1", "Wren went down."),
				document("Chapter 2", "Nobody was there."),
				document("Chapter 3", "Wren again, and Wren."),
			],
			"wren",
		);

		expect(results.groups.map((group) => group.title)).toEqual([
			"Chapter 1",
			"Chapter 3",
		]);
		expect(results.groups.map((group) => group.count)).toEqual([1, 2]);
	});

	test("documents keep the order they were given", () => {
		const results = search(
			[document("B", "Wren"), document("A", "Wren")],
			"wren",
		);
		expect(results.groups.map((group) => group.title)).toEqual(["B", "A"]);
	});

	test("matching ignores case, in the text and in the name", () => {
		const results = search([document("WREN", "wren")], "Wren");
		expect(results.groups[0].titleHit).toBe(true);
		expect(results.groups[0].hits).toHaveLength(1);
	});

	test("a query shorter than the minimum finds nothing at all", () => {
		const documents = [document("Chapter 1", "aaa"), unreadable("Gone")];
		expect(search(documents, "a".repeat(MIN_QUERY - 1))).toEqual({
			groups: [],
			unreadable: [],
		});
	});
});

describe("hits", () => {
	test("each one carries the block it sits in and where it is", () => {
		const results = search([document("Ch", "Wren went down.")], "went");
		expect(results.groups[0].hits).toEqual([
			{ ordinal: 0, line: "Wren went down.", from: 5, to: 9 },
		]);
	});

	test("the offsets survive formatting inside the block", () => {
		const results = search(
			[document("Ch", "She was the **best** of them.")],
			"best",
		);
		const [hit] = results.groups[0].hits;

		expect(hit.line).toBe("She was the best of them.");
		expect(hit.line.slice(hit.from, hit.to)).toBe("best");
	});

	test("a match spanning a formatting boundary keeps its whole span", () => {
		const results = search(
			[document("Ch", "She was the **best** of them.")],
			"the best",
		);
		const [hit] = results.groups[0].hits;

		expect(hit.line.slice(hit.from, hit.to)).toBe("the best");
	});

	test("ordinals count from zero, in reading order", () => {
		const results = search(
			[
				document(
					"Ch",
					"# Wren\n\nWren went out. Later, *Wren* came back.",
				),
			],
			"wren",
		);
		const hits = results.groups[0].hits;

		expect(hits.map((hit) => hit.ordinal)).toEqual([0, 1, 2]);
		expect(hits[0].line).toBe("Wren");
		expect(hits[1].line).toBe("Wren went out. Later, Wren came back.");
		expect(hits[2].line).toBe(hits[1].line);
		expect(hits[1].from).toBeLessThan(hits[2].from);
	});

	test("a hit in a list item is bounded by that item", () => {
		const results = search([document("Ch", "- the end\n- the sea")], "the");
		const hits = results.groups[0].hits;

		expect(hits.map((hit) => hit.line)).toEqual(["the end", "the sea"]);
		expect(hits.map((hit) => hit.from)).toEqual([0, 0]);
	});
});

describe("a document whose name matches", () => {
	test("comes back even with nothing in its prose", () => {
		const results = search([document("Wren", "Nobody was there.")], "wren");

		expect(results.groups[0].titleHit).toBe(true);
		expect(results.groups[0].hits).toEqual([]);
		expect(results.groups[0].count).toBe(1);
	});

	test("counts alongside its prose hits", () => {
		const results = search([document("Wren", "Wren went down.")], "wren");

		expect(results.groups[0].count).toBe(2);
		expect(results.groups[0].hits).toHaveLength(1);
	});

	test("does not shift the ordinals its prose hits are stepped to", () => {
		const results = search([document("Wren", "Wren. Wren.")], "wren");
		expect(results.groups[0].hits.map((hit) => hit.ordinal)).toEqual([
			0, 1,
		]);
	});
});

describe("a document that could not be read", () => {
	test("is named rather than left out", () => {
		const results = search(
			[document("Chapter 1", "Wren went down."), unreadable("Chapter 2")],
			"wren",
		);

		expect(results.groups.map((group) => group.title)).toEqual([
			"Chapter 1",
		]);
		expect(results.unreadable.map((entry) => entry.title)).toEqual([
			"Chapter 2",
		]);
	});

	test("is named even when the query is nowhere else", () => {
		const results = search([unreadable("Chapter 2")], "wren");

		expect(results.groups).toEqual([]);
		expect(results.unreadable).toHaveLength(1);
	});
});
