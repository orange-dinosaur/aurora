import { describe, expect, it } from "vitest";
import { differences } from "../differences";
import type { TakeView } from "../bindings/TakeView";
import { baseline, blocks, change, follow, marked, tally } from "./compare";

const plain = { strong: false, em: false, difference: null };

describe("marked", () => {
	function page(live: string, prose: string) {
		const { body, cut } = marked(live, prose);
		return body.map((block) =>
			block.kind === "break"
				? "---"
				: block.runs
						.map(({ text, difference }) => {
							if (difference === null) {
								return text;
							}
							return cut.has(difference)
								? `[-${text}-]`
								: `[+${text}+]`;
						})
						.join(""),
		);
	}

	it("sets cut words just before what replaced them", () => {
		expect(page("The rain came down.", "The rain fell softly.")).toEqual([
			"The rain [-fell softly-] [+came down+].",
		]);
	});

	it("keeps words that were only cut or only added in their places", () => {
		expect(page("One two.", "One big two.")).toEqual(["One [-big -]two."]);
		expect(page("One big two.", "One two.")).toEqual(["One [+big +]two."]);
	});

	it("gives a cut paragraph its own block", () => {
		expect(page("One.\n\nThree.", "One.\n\nTwo.\n\nThree.")).toEqual([
			"One.",
			"[-Two.-]",
			"Three.",
		]);
	});

	it("leaves the front matter out of it", () => {
		const head = "---\nsynopsis: A\n---\n\n";
		expect(page(`${head}Same words.`, "Same words.")).toEqual([
			"Same words.",
		]);
	});

	it("ignores space at either end of the prose", () => {
		expect(page("Same words.\n", "\nSame words.")).toEqual(["Same words."]);
	});

	it("drops the markers of cut words, so they do not flip what follows", () => {
		const { body } = marked("*One* two three", "*One two* three");
		expect(
			body.flatMap((block) =>
				block.kind === "break"
					? []
					: block.runs.filter((run) => run.em).map((run) => run.text),
			),
		).toEqual(["One"]);
	});
});

describe("baseline", () => {
	function take(id: string, rest: Partial<TakeView> = {}): TakeView {
		return {
			id,
			name: id,
			from: null,
			chosen: false,
			words: 0,
			...rest,
		};
	}

	it("is the take the chosen one started from", () => {
		expect(
			baseline([
				take("a"),
				take("b"),
				take("c", { from: "a", chosen: true }),
			]),
		).toBe("a");
	});

	it("is the take to the left once that one is gone", () => {
		expect(
			baseline([
				take("a"),
				take("b"),
				take("c", { from: "gone", chosen: true }),
			]),
		).toBe("b");
	});

	it("is nothing on the first take with no start", () => {
		expect(baseline([take("a", { chosen: true }), take("b")])).toBeNull();
	});
});

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
