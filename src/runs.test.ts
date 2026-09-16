import { describe, expect, test } from "vitest";
import { matches } from "./lib/find";
import { runsOf } from "./runs";

/** The text of each run, which is what matching reads. */
function texts(markdown: string): string[] {
	return runsOf(markdown).map((run) => run.text);
}

/** The runs grouped by the block they sit in, in reading order. */
function blocks(markdown: string): string[][] {
	const grouped: string[][] = [];
	let previous: string | null = null;

	for (const run of runsOf(markdown)) {
		if (run.block !== previous) {
			grouped.push([]);
			previous = run.block;
		}
		grouped[grouped.length - 1].push(run.text);
	}

	return grouped;
}

describe("a document becomes the runs the editor would hold", () => {
	test("plain prose is one run in one block", () => {
		expect(blocks("She went down to the water.")).toEqual([
			["She went down to the water."],
		]);
	});

	test("each paragraph is its own block", () => {
		expect(blocks("One.\n\nTwo.")).toEqual([["One."], ["Two."]]);
	});

	test("a heading is a block like any other", () => {
		expect(blocks("# Chapter One\n\nShe ran.")).toEqual([
			["Chapter One"],
			["She ran."],
		]);
	});

	test("emphasis splits a paragraph into several runs", () => {
		expect(texts("She was *late*, and **very** cross.")).toEqual([
			"She was ",
			"late",
			", and ",
			"very",
			" cross.",
		]);
	});

	test("the markers themselves are not in the text", () => {
		expect(texts("**bold**").join("")).toBe("bold");
	});

	// Every item of a list shares one top-level element, so the block a run
	// belongs to cannot simply be that.
	test("a list item is a block, so a query cannot run between two", () => {
		expect(blocks("- one\n- two")).toEqual([["one"], ["two"]]);
		expect(matches(runsOf("- the end\n- the beginning"), "endthe")).toEqual(
			[],
		);
	});

	test("a nested list item is a block of its own too", () => {
		expect(blocks("- one\n    - inner\n- two")).toEqual([
			["one"],
			["inner"],
			["two"],
		]);
	});

	test("front matter is not in the document at all", () => {
		const markdown = "---\ntitle: Ithaca\n---\n\nShe ran.";
		expect(texts(markdown)).toEqual(["She ran."]);
	});

	test("an empty document has no runs", () => {
		expect(runsOf("")).toEqual([]);
	});
});

// The reason matching was put in the frontend rather than in Rust. Every case
// here is one where searching the file as text gives a different answer from
// searching what the writer can see.
describe("matching reads what the editor shows, not what the file says", () => {
	test("a phrase is found across a formatting boundary", () => {
		const markdown = "She was the **best** of them.";
		expect(markdown).not.toContain("the best");
		expect(matches(runsOf(markdown), "the best")).toHaveLength(1);
	});

	test("a markdown marker is not findable", () => {
		expect(matches(runsOf("**bold**"), "**")).toHaveLength(0);
	});

	test("a heading's hashes are not findable", () => {
		const runs = runsOf("## Chapter One");
		expect(matches(runs, "Chapter")).toHaveLength(1);
		expect(matches(runs, "## Chapter")).toHaveLength(0);
	});

	test("front matter is not searchable", () => {
		const runs = runsOf("---\ntitle: Ithaca\n---\n\nShe ran.");
		expect(matches(runs, "Ithaca")).toHaveLength(0);
	});

	test("a query does not run from one paragraph into the next", () => {
		expect(matches(runsOf("One.\n\nTwo."), "One. Two.")).toHaveLength(0);
	});

	test("every occurrence is found, in reading order", () => {
		const markdown = "# Wren\n\nWren went out. Later, *Wren* came back.";
		expect(matches(runsOf(markdown), "wren")).toHaveLength(3);
	});
});
