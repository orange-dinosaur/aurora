import { describe, expect, test } from "vitest";
import { differences, putBack, type Difference } from "./differences";

/** Each difference as the text it covers then and now. */
function shown(then: string, now: string) {
	return differences(then, now).map((difference: Difference) => ({
		...(difference.kind === "field" ? { key: difference.key } : {}),
		then: then.slice(difference.then.from, difference.then.to),
		now: now.slice(difference.now.from, difference.now.to),
	}));
}

/** Puts back the first difference until none are left, and returns the text. */
function putEverythingBack(then: string, now: string): string {
	let text = now;
	for (let round = 0; round < 50; round++) {
		const [first] = differences(then, text);
		if (first === undefined) {
			return text;
		}
		text = putBack(text, then, first);
	}
	throw new Error("putting back never finished");
}

const SCENE = "---\nsynopsis: A storm\ntags: [sea, night]\n---\n";

describe("the body", () => {
	test("the same text has no differences", () => {
		expect(differences(SCENE + "Calm.", SCENE + "Calm.")).toEqual([]);
	});

	test("changed words with only space between them are one difference", () => {
		expect(shown("the cold wind blew", "the warm rain blew")).toEqual([
			{ then: "cold wind", now: "warm rain" },
		]);
	});

	test("an unchanged word keeps two edits apart", () => {
		expect(shown("a cold and wet day", "a warm and dry day")).toEqual([
			{ then: "cold", now: "warm" },
			{ then: "wet", now: "dry" },
		]);
	});

	test("a paragraph break keeps two edits apart", () => {
		expect(shown("Cold\n\nWet", "Warm\n\nDry")).toEqual([
			{ then: "Cold", now: "Warm" },
			{ then: "Wet", now: "Dry" },
		]);
	});

	test("a changed word is one difference", () => {
		expect(shown("The cat sat.", "The dog sat.")).toEqual([
			{ then: "cat", now: "dog" },
		]);
	});

	test("a word changed inside emphasis leaves the markers alone", () => {
		expect(shown("A *very big* dog.", "A *very large* dog.")).toEqual([
			{ then: "big", now: "large" },
		]);
	});

	test("emphasis added widens to the word it covers", () => {
		expect(shown("Hello world.", "Hello *world*.")).toEqual([
			{ then: "world", now: "*world*" },
		]);
	});

	test("a marker that moves takes the whole emphasis with it", () => {
		expect(shown("*one two* three", "*one* two three")).toEqual([
			{ then: "*one two*", now: "*one* two" },
		]);
	});

	test("an underscore inside a word is not emphasis", () => {
		expect(shown("a snake_case name", "a snake_case label")).toEqual([
			{ then: "name", now: "label" },
		]);
	});
});

describe("the front matter", () => {
	test("each changed field is its own difference", () => {
		const now = "---\nsynopsis: A calm\ntags: [sea, day]\n---\n";

		expect(shown(SCENE + "Rain.", now + "Rain.")).toEqual([
			{
				key: "synopsis",
				then: "synopsis: A storm\n",
				now: "synopsis: A calm\n",
			},
			{
				key: "tags",
				then: "tags: [sea, night]\n",
				now: "tags: [sea, day]\n",
			},
		]);
	});

	test("a field added and a field removed", () => {
		const now = "---\nsynopsis: A storm\npov: Ann\n---\n";

		expect(shown(SCENE, now)).toEqual([
			{ key: "tags", then: "tags: [sea, night]\n", now: "" },
			{ key: "pov", then: "", now: "pov: Ann\n" },
		]);
	});

	test("a field changed and a word changed are told apart", () => {
		const now = "---\nsynopsis: A calm\ntags: [sea, night]\n---\n";

		expect(shown(SCENE + "Rain fell.", now + "Snow fell.")).toEqual([
			{
				key: "synopsis",
				then: "synopsis: A storm\n",
				now: "synopsis: A calm\n",
			},
			{ then: "Rain", now: "Snow" },
		]);
	});
});

describe("putting back", () => {
	test("one difference leaves the others as they are", () => {
		const then = "The cat sat on the mat.";
		const now = "The dog sat on the rug.";
		const second = differences(then, now)[1];

		expect(putBack(now, then, second)).toBe("The dog sat on the mat.");
	});

	test.each([
		["words", "The cat sat on the mat.", "A dog lay on the warm rug!"],
		["emphasis", "*One two* three _four_.", "One *two three* four."],
		[
			"fields",
			SCENE + "Body.",
			"---\npov: Ann\nsynopsis: Calm\n---\nBody.",
		],
		["front matter added", "Only prose.", SCENE + "Only prose."],
		["front matter removed", SCENE + "Only prose.", "Only prose."],
		["paragraphs", "One.\n\nTwo *three*.\n", "One.\n\n\nTwo three.\nFour."],
		[
			"windows lines",
			"---\r\nsynopsis: A\r\n---\r\nText.",
			"---\r\nsynopsis: B\r\n---\r\nText.",
		],
		["empty", "", "Something new."],
		["emptied", "Something old.", ""],
	])("everything put back gives the old text: %s", (_, then, now) => {
		expect(putEverythingBack(then, now)).toBe(then);
	});
});
