import { describe, expect, it } from "vitest";
import type { Conflict, Piece, TreeNode } from "../types";
import {
	cardKey,
	cards,
	cardsLeft,
	fieldText,
	fieldTitle,
	joined,
	left,
	leftLabel,
	named,
	nextOpen,
	paragraphs,
	settled,
	spotCount,
	spotsLeft,
	theirSides,
} from "./merge";

const pieces: Piece[] = [
	{ kind: "clean", text: "One.\n\n" },
	{ kind: "spot", ours: "Two, mine.\n", theirs: "Two, theirs.\n" },
	{ kind: "clean", text: "\nThree.\n\n" },
	{ kind: "spot", ours: "Four, mine.\n", theirs: "Four, theirs.\n" },
];

describe("settled", () => {
	const spot = { ours: "Mine.\n", theirs: "Theirs.\n" };

	it("takes one side whole", () => {
		expect(settled(spot, "ours")).toBe("Mine.\n");
		expect(settled(spot, "theirs")).toBe("Theirs.\n");
	});

	it("keeps both as paragraphs of their own, this version's first", () => {
		expect(settled(spot, "both")).toBe("Mine.\n\nTheirs.\n");
		expect(settled({ ours: "Mine.", theirs: "Theirs." }, "both")).toBe(
			"Mine.\n\nTheirs.",
		);
		expect(settled({ ours: "Mine.\n\n", theirs: "Theirs." }, "both")).toBe(
			"Mine.\n\nTheirs.",
		);
	});

	it("keeps both without a gap when one side is empty", () => {
		expect(settled({ ours: "", theirs: "Theirs.\n" }, "both")).toBe(
			"Theirs.\n",
		);
		expect(settled({ ours: "Mine.\n", theirs: "" }, "both")).toBe(
			"Mine.\n",
		);
	});
});

describe("joined", () => {
	it("puts the clean text and each settled spot back in order", () => {
		expect(joined(pieces, ["theirs", "both"])).toBe(
			"One.\n\nTwo, theirs.\n\nThree.\n\nFour, mine.\n\nFour, theirs.\n",
		);
	});

	it("is null while a spot is still open", () => {
		expect(joined(pieces, ["ours", null])).toBeNull();
		expect(joined(pieces, ["ours"])).toBeNull();
	});
});

describe("what is left", () => {
	it("counts the spots not chosen yet", () => {
		expect(spotCount(pieces)).toBe(2);
		expect(left(pieces, [])).toBe(2);
		expect(left(pieces, [null, "ours"])).toBe(1);
		expect(left(pieces, ["ours", "theirs", "ours"])).toBe(0);
	});

	it("words the list's count, and says when it is settled", () => {
		expect(leftLabel(1, 1)).toBe("1 left");
		expect(leftLabel(6, 6)).toBe("6 left");
		expect(leftLabel(2, 3)).toBe("2 of 3 left");
		expect(leftLabel(0, 3)).toBe("Settled");
	});

	it("words the heading's count", () => {
		expect(spotsLeft(2, 3)).toBe("2 of 3 spots left");
		expect(spotsLeft(1, 1)).toBe("1 of 1 spot left");
		expect(spotsLeft(0, 3)).toBe("Every spot is settled");
		expect(spotsLeft(0, 1)).toBe("The spot is settled");
	});
});

describe("nextOpen", () => {
	it("finds the next spot to choose, going round to the start", () => {
		expect(nextOpen([null, null, null], 3, 0)).toBe(1);
		expect(nextOpen(["ours", null, "ours"], 3, 2)).toBe(1);
		expect(nextOpen([null, "ours", "ours"], 3, 0)).toBe(0);
	});

	it("starts from the first with -1, and gives -1 once all are chosen", () => {
		expect(nextOpen([], 2, -1)).toBe(0);
		expect(nextOpen(["ours", "both"], 2, 0)).toBe(-1);
		expect(nextOpen([], 0, -1)).toBe(-1);
	});
});

describe("the binder's cards", () => {
	const conflicts: Conflict[] = [
		{ kind: "text", id: "d1", path: "Manuscript/Scene 2.md" },
		{ kind: "renamed", id: "n1", ours: "Rain", theirs: "The letter" },
		{ kind: "field", id: null, field: "title", ours: "A", theirs: "B" },
		{ kind: "field", id: null, field: "author", ours: "C", theirs: "D" },
		{ kind: "cover", ours: "winter.jpg", theirs: null },
	];
	const shown = cards(conflicts);

	it("are every conflict outside the text", () => {
		expect(shown.map((card) => card.kind)).toEqual([
			"renamed",
			"field",
			"field",
			"cover",
		]);
	});

	it("are told apart by kind, id and field, never by value", () => {
		const keys = shown.map(cardKey);
		expect(new Set(keys).size).toBe(4);
		expect(
			cardKey({
				kind: "field",
				id: null,
				field: "title",
				ours: 1,
				theirs: 2,
			}),
		).toBe(keys[1]);
	});

	it("settle to nothing while one is open", () => {
		const sides = {
			[cardKey(shown[0])]: "theirs",
			[cardKey(shown[1])]: "ours",
			[cardKey(shown[2])]: "theirs",
		} as const;
		expect(cardsLeft(shown, sides)).toBe(1);
		expect(theirSides(shown, sides)).toBeNull();
	});

	it("settle to the ones chosen the other version's way", () => {
		const sides = {
			[cardKey(shown[0])]: "theirs",
			[cardKey(shown[1])]: "ours",
			[cardKey(shown[2])]: "theirs",
			[cardKey(shown[3])]: "ours",
		} as const;
		expect(cardsLeft(shown, sides)).toBe(0);
		expect(theirSides(shown, sides)).toEqual([shown[0], shown[2]]);
	});

	it("settle to nothing chosen theirs when every card keeps mine", () => {
		const sides = Object.fromEntries(
			shown.map((card) => [cardKey(card), "ours" as const]),
		);
		expect(theirSides(shown, sides)).toEqual([]);
	});
});

describe("named", () => {
	it("names every node and the folder it sits in", () => {
		const tree: TreeNode[] = [
			{
				node: "folder",
				id: "f1",
				name: "Chapter 1",
				kind: null,
				words: 0,
				inBook: true,
				children: [
					{
						node: "document",
						id: "d1",
						path: "Chapter 1/Scene 1.md",
						trail: ["Chapter 1"],
						title: "Scene 1",
						target: null,
						inBook: true,
					},
				],
			},
		];
		const names = named(tree);
		expect(names.get("f1")).toEqual({ name: "Chapter 1", folder: "" });
		expect(names.get("d1")).toEqual({
			name: "Scene 1",
			folder: "Chapter 1",
		});
	});
});

describe("fields", () => {
	it("are titled by what Aurora calls them, or as the writer typed them", () => {
		expect(fieldTitle("seriesNumber", null)).toBe(
			"Series number of the book",
		);
		expect(fieldTitle("target", "Chapter 2")).toBe("Target of Chapter 2");
		expect(fieldTitle("POV", "Chapter 2")).toBe("POV of Chapter 2");
	});

	it("read as text, lists joined and nothing empty", () => {
		expect(fieldText("Wintering")).toBe("Wintering");
		expect(fieldText(2000)).toBe("2000");
		expect(fieldText(null)).toBe("");
		expect(fieldText(["winter", "", "thaw"])).toBe("winter, thaw");
		expect(fieldText([{ name: "Ana", role: "translator" }])).toBe(
			"Ana, translator",
		);
	});
});

describe("paragraphs", () => {
	it("parts text on blank lines and drops the empty ones", () => {
		expect(paragraphs("\nOne.\nstill one.\n\n \nTwo.\n\n")).toEqual([
			"One.\nstill one.",
			"Two.",
		]);
	});
});
