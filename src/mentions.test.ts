import { describe, expect, test } from "vitest";
import { matches } from "./find";
import { type Subject, mentions } from "./mentions";
import { runsOf } from "./runs";

function subject(title: string, ...names: string[]): Subject {
	return { path: `Characters/${title}.md`, title, names };
}

/** The names recognised in `markdown`, in the order they were met. */
function found(subjects: Subject[], markdown: string): string[] {
	return mentions(subjects, runsOf(markdown)).map((mention) => mention.name);
}

describe("a whole-word query stops at the edges of a word", () => {
	const reading = { wholeWord: true };

	test("a longer word is not a match", () => {
		expect(
			matches(runsOf("Rosemary waited."), "Rose", reading),
		).toHaveLength(0);
		expect(
			matches(runsOf("She read the prose."), "rose", reading),
		).toHaveLength(0);
	});

	test("punctuation and quotes are edges", () => {
		const markdown = '"Rose," he said. Rose\'s hand. (Rose)';
		expect(matches(runsOf(markdown), "Rose", reading)).toHaveLength(3);
	});

	test("a rejected place does not hide the match just after it", () => {
		// Skipping the width of the query would step over the second `and`.
		expect(matches(runsOf("brand and rand"), "and", reading)).toHaveLength(
			1,
		);
	});

	test("Find is unchanged, because it does not ask for this", () => {
		expect(matches(runsOf("Rosemary waited."), "Rose")).toHaveLength(1);
	});
});

describe("recognition tells one name from another", () => {
	test("a subject with no extra names still matches its own title", () => {
		expect(found([subject("Elena")], "Elena went out.")).toEqual(["Elena"]);
	});

	test("case matters, unlike a search", () => {
		expect(found([subject("Rose")], "A rose in the window.")).toEqual([]);
		expect(matches(runsOf("A rose in the window."), "Rose")).toHaveLength(
			1,
		);
	});

	test("a name inside a longer word is not the subject", () => {
		const text = "Rosemary read the prose while Rose waited.";
		expect(found([subject("Rose")], text)).toEqual(["Rose"]);
	});

	test("an extra name counts as much as the title", () => {
		const text = "She watched the Captain board. Elena did not look up.";
		expect(found([subject("Elena", "the Captain")], text)).toEqual([
			"the Captain",
			"Elena",
		]);
	});

	test("a name is found across a formatting boundary", () => {
		const markdown = "She saw the **Captain** there.";
		expect(markdown).not.toContain("the Captain");
		expect(found([subject("Elena", "the Captain")], markdown)).toEqual([
			"the Captain",
		]);
	});

	test("a name does not run from one paragraph into the next", () => {
		const markdown = "She left the\n\nCaptain waited.";
		expect(found([subject("Elena", "the Captain")], markdown)).toEqual([]);
	});

	test("blank and repeated names are dropped", () => {
		const text = "Elena went out.";
		expect(found([subject("Elena", "", "  ", "Elena")], text)).toEqual([
			"Elena",
		]);
	});
});

describe("mentions come back in reading order, whoever they belong to", () => {
	test("subjects are interleaved by where they appear", () => {
		const text = "Elena spoke. Rose answered. Elena left.";
		expect(found([subject("Rose"), subject("Elena")], text)).toEqual([
			"Elena",
			"Rose",
			"Elena",
		]);
	});

	test("a mention carries the subject it belongs to", () => {
		const runs = runsOf("Rose answered.");
		expect(mentions([subject("Rose")], runs)[0].subject).toBe(
			"Characters/Rose.md",
		);
	});

	test("two subjects sharing a name are both reported", () => {
		const both = [
			subject("Rose"),
			{ ...subject("Rosewater"), names: ["Rose"] },
		];
		expect(found(both, "Rose stood there.")).toHaveLength(2);
	});

	test("the longer of two overlapping names is reported first", () => {
		const rose = subject("Rose", "Rose Quartz");
		expect(found([rose], "Rose Quartz turned.")).toEqual([
			"Rose Quartz",
			"Rose",
		]);
	});
});
