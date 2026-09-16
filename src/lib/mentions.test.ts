import { describe, expect, test } from "vitest";
import { matches } from "./find";
import {
	appearancesIn,
	castOf,
	type Mentionable,
	type Subject,
	mentions,
	mentionsIn,
	subjectsIn,
	under,
} from "./mentions";
import { runsOf } from "../runs";

function subject(title: string, ...names: string[]): Subject {
	return { id: `Characters/${title}.md`, title, names };
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

function chapter(
	title: string,
	markdown: string,
	tags: string[] = [],
	trail: string[] = ["Manuscript"],
): Mentionable {
	return {
		id: `${trail.join("/")}/${title}.md`,
		title,
		trail,
		tags,
		names: [],
		runs: runsOf(markdown),
	};
}

/** A character page as the corpus reads it. */
function page(title: string, names: string[] = []): Mentionable {
	return {
		id: `Characters/${title}.md`,
		title,
		trail: ["Characters"],
		tags: [],
		names,
		runs: runsOf(`# ${title}`),
	};
}

const elena = subject("Elena", "the Captain");

describe("a subject's page collects where it is spoken of", () => {
	test("a document that names it and one that tags it both appear", () => {
		const { groups } = mentionsIn(elena, [
			chapter("One", "Elena went out."),
			chapter("Two", "The door stayed shut.", ["Elena"]),
			chapter("Three", "Nobody was there."),
		]);

		expect(groups.map((group) => group.title)).toEqual(["One", "Two"]);
		expect(groups[0].hits.map((hit) => hit.name)).toEqual(["Elena"]);
		expect(groups[0].tagged).toEqual([]);
		expect(groups[1].hits).toEqual([]);
		expect(groups[1].tagged).toEqual(["Elena"]);
	});

	test("a tag only counts when it is one of the subject's names", () => {
		const { groups } = mentionsIn(elena, [
			chapter("One", "Nothing here.", ["elena", "sea"]),
		]);

		expect(groups).toEqual([]);
	});

	test("the subject's own page is left out", () => {
		const own: Mentionable = {
			id: elena.id,
			title: elena.title,
			trail: ["Characters"],
			tags: [],
			names: [],
			runs: runsOf("Elena is the Captain."),
		};

		expect(mentionsIn(elena, [own]).groups).toEqual([]);
	});

	test("a hit carries the line it sits in and where it is in it", () => {
		const { groups } = mentionsIn(elena, [
			chapter("One", "The wind rose. Elena went out."),
		]);
		const hit = groups[0].hits[0];

		expect(hit.line).toBe("The wind rose. Elena went out.");
		expect(hit.line.slice(hit.from, hit.to)).toBe("Elena");
	});

	test("each name is numbered on its own, for Find to step to", () => {
		const { groups } = mentionsIn(elena, [
			chapter(
				"One",
				"Elena spoke. the Captain turned. Elena left the Captain.",
			),
		]);

		expect(groups[0].hits.map((hit) => [hit.name, hit.ordinal])).toEqual([
			["Elena", 0],
			["the Captain", 0],
			["Elena", 1],
			["the Captain", 1],
		]);
	});

	test("tags and hits are counted together", () => {
		const { groups } = mentionsIn(elena, [
			chapter("One", "Elena spoke. Elena left.", ["Elena"]),
		]);

		expect(groups[0].count).toBe(3);
	});

	test("a document that could not be read is reported, not skipped", () => {
		const { groups, unreadable } = mentionsIn(elena, [
			{ ...chapter("One", ""), runs: null },
		]);

		expect(groups).toEqual([]);
		expect(unreadable.map((document) => document.title)).toEqual(["One"]);
	});
});

describe("a document collects the subjects it speaks of", () => {
	const cast = [page("Elena", ["the Captain"]), page("Rose"), page("Wren")];

	test("only the pages under a subject section count as subjects", () => {
		const documents = [...cast, chapter("One", "Elena went out.")];

		expect(subjectsIn(documents).map((subject) => subject.title)).toEqual([
			"Elena",
			"Rose",
			"Wren",
		]);
	});

	test("subjects come in the order they are first named, with counts", () => {
		const here = chapter(
			"One",
			"Rose spoke. Elena answered. Rose left, and the Captain with her.",
		);

		expect(
			appearancesIn(here, subjectsIn(cast)).map(({ subject, count }) => [
				subject.title,
				count,
			]),
		).toEqual([
			["Rose", 2],
			["Elena", 2],
		]);
	});

	test("a subject the document only tags comes last, uncounted", () => {
		const here = chapter("One", "Rose spoke.", ["Wren"]);

		expect(
			appearancesIn(here, subjectsIn(cast)).map(
				({ subject, count, tagged }) => [subject.title, count, tagged],
			),
		).toEqual([
			["Rose", 1, false],
			["Wren", 0, true],
		]);
	});

	test("a document that both names and tags a subject says so once", () => {
		const here = chapter("One", "Rose spoke.", ["Rose"]);

		expect(appearancesIn(here, subjectsIn(cast))).toEqual([
			{
				subject: { id: cast[1].id, title: "Rose", names: [] },
				count: 1,
				tagged: true,
			},
		]);
	});

	test("a subject's own page does not appear in its own list", () => {
		const own = { ...cast[1], runs: runsOf("Rose is a gardener.") };

		expect(appearancesIn(own, subjectsIn(cast))).toEqual([]);
	});

	test("a document that could not be read names nobody", () => {
		const here = { ...chapter("One", ""), runs: null };

		expect(appearancesIn(here, subjectsIn(cast))).toEqual([]);
	});
});

describe("a folder collects the cast of everything under it", () => {
	const cast = [page("Elena", ["the Captain"]), page("Rose"), page("Wren")];
	const part = ["Manuscript", "Part One"];

	const chapters = [
		chapter("One", "Elena spoke. Elena left. Rose waited.", [], part),
		chapter("Two", "She watched the Captain turn.", [], part),
		chapter("Three", "Wren went home.", ["Rose"], ["Manuscript"]),
	];

	test("a folder's documents are the ones its trail runs through", () => {
		expect(
			under([...cast, ...chapters], part).map(
				(document) => document.title,
			),
		).toEqual(["One", "Two"]);
	});

	test("a section takes in the folders below it", () => {
		expect(
			under(chapters, ["Manuscript"]).map((document) => document.title),
		).toEqual(["One", "Two", "Three"]);
	});

	test("a document counts once however often it names a subject", () => {
		const counts = castOf(under(chapters, part), subjectsIn(cast));

		expect(
			counts.map(({ subject, count }) => [subject.title, count]),
		).toEqual([
			["Elena", 2],
			["Rose", 1],
		]);
	});

	test("the most widely present comes first, and ties go by name", () => {
		const counts = castOf(chapters, subjectsIn(cast));

		expect(
			counts.map(({ subject, count }) => [subject.title, count]),
		).toEqual([
			["Elena", 2],
			["Rose", 2],
			["Wren", 1],
		]);
	});

	test("a document that only tags a subject still counts for it", () => {
		const counts = castOf(
			[chapter("Three", "Nobody here.", ["Rose"])],
			subjectsIn(cast),
		);

		expect(
			counts.map(({ subject, count }) => [subject.title, count]),
		).toEqual([["Rose", 1]]);
	});

	test("a folder nobody is named in has no cast", () => {
		expect(castOf(under(chapters, ["Notes"]), subjectsIn(cast))).toEqual(
			[],
		);
	});
});
