import { describe, expect, it } from "vitest";
import type { Piece } from "../types";
import {
	joined,
	left,
	leftLabel,
	nextOpen,
	paragraphs,
	settled,
	spotCount,
	spotsLeft,
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

describe("paragraphs", () => {
	it("parts text on blank lines and drops the empty ones", () => {
		expect(paragraphs("\nOne.\nstill one.\n\n \nTwo.\n\n")).toEqual([
			"One.\nstill one.",
			"Two.",
		]);
	});
});
