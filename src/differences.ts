// What differs between two texts of one document, for the compare view to
// draw and for putting back one difference at a time.

import { diffArrays } from "diff";
import { segments, split } from "./frontmatter";

/** Where a difference sits in one of the two texts, as character offsets. */
export type Span = { from: number; to: number };

/**
 * One difference between a document's text then and its text now. A field
 * difference is one field of the front matter, whose key is null for a line
 * that is not a field, such as a fence. A body difference is a run of words.
 */
export type Difference =
	| { kind: "field"; key: string | null; then: Span; now: Span }
	| { kind: "body"; then: Span; now: Span };

type Token = { text: string; key: string | null; field: boolean };

// A run of emphasis markers, which is a token of its own so that a changed
// word never takes the marker next to it with it.
const MARKER = /^[*_]+$/;
const WHITESPACE = /^\s+$/;
const PARAGRAPH_BREAK = /\n[^\S\n]*\n/;

// Space, emphasis markers, words with their apostrophes, and punctuation, each
// a token, so a word reads the same whatever sits next to it.
const WORDS = /\s+|[*_]+|[\p{L}\p{N}'’]+|[^\s*_\p{L}\p{N}'’]+/gu;

function tokens(text: string): Token[] {
	const { block, body } = split(text);
	const head = text.slice(0, text.length - body.length);
	const words = (body.match(WORDS) ?? []).map((word) => ({
		text: word,
		key: null,
		field: false,
	}));

	return [...fieldTokens(block, head), ...words];
}

// The front matter one field at a time, or as one piece when its lines do not
// join back into exactly what the file holds.
function fieldTokens(block: string, head: string): Token[] {
	if (head === "") {
		return [];
	}

	const open = "---\n";
	const fields = segments(block).map((segment) => ({
		text: segment.text.map((line) => `${line}\n`).join(""),
		key: segment.key,
		field: true,
	}));
	const inside = fields.map((field) => field.text).join("");
	const close = head.slice(open.length + inside.length);
	const pieces = [
		{ text: open, key: null, field: true },
		...fields,
		{ text: close, key: null, field: true },
	];

	return head.startsWith(open) && open + inside + close === head
		? pieces.filter((piece) => piece.text !== "")
		: [{ text: head, key: null, field: true }];
}

// A stretch of the alignment: one token both sides share, or a run that
// differs. Ranges are token indices, end excluded.
type Cell = { then: [number, number]; now: [number, number]; same: boolean };

function align(then: Token[], now: Token[]): Cell[] {
	const label = (token: Token) => (token.field ? "f" : "b") + token.text;
	const cells: Cell[] = [];
	let i = 0;
	let j = 0;

	for (const change of diffArrays(then.map(label), now.map(label))) {
		if (!change.added && !change.removed) {
			for (let k = 0; k < change.count; k++) {
				cells.push({ then: [i, i + 1], now: [j, j + 1], same: true });
				i++;
				j++;
			}
			continue;
		}

		let last = cells[cells.length - 1];
		if (last === undefined || last.same) {
			last = { then: [i, i], now: [j, j], same: false };
			cells.push(last);
		}
		if (change.removed) {
			i += change.count;
			last.then[1] = i;
		} else {
			j += change.count;
			last.now[1] = j;
		}
	}

	return cells;
}

// The indices in a range, from one side, whose tokens pass a test.
function where(
	tokens: Token[],
	[from, to]: [number, number],
	test: (token: Token) => boolean,
): number[] {
	const found: number[] = [];
	for (let k = from; k < to; k++) {
		if (test(tokens[k])) {
			found.push(k);
		}
	}
	return found;
}

function range(found: number[], empty: number): [number, number] {
	return found.length === 0
		? [empty, empty]
		: [found[0], found[found.length - 1] + 1];
}

// A changed run in the front matter, cut into one run per field it touches.
function byField(cell: Cell, then: Token[], now: Token[]) {
	const keys = new Set([
		...where(then, cell.then, (t) => t.field).map((k) => then[k].key),
		...where(now, cell.now, (t) => t.field).map((k) => now[k].key),
	]);

	return [...keys].map((key) => ({
		key,
		then: range(
			where(then, cell.then, (t) => t.field && t.key === key),
			cell.then[0],
		),
		now: range(
			where(now, cell.now, (t) => t.field && t.key === key),
			cell.now[0],
		),
	}));
}

/**
 * Each emphasis marker in the body, paired with the one that closes it. A
 * marker with space on both sides opens nothing, and an underscore inside a
 * word is part of the word. Emphasis never runs past a paragraph.
 */
function partners(tokens: Token[]): Map<number, number> {
	const pairs = new Map<number, number>();
	let open: number[] = [];
	const spaced = (k: number) =>
		k < 0 || k >= tokens.length || WHITESPACE.test(tokens[k].text);

	tokens.forEach((token, k) => {
		if (token.field) {
			return;
		}
		if (PARAGRAPH_BREAK.test(token.text)) {
			open = [];
			return;
		}
		if (!MARKER.test(token.text)) {
			return;
		}
		const [before, after] = [spaced(k - 1), spaced(k + 1)];
		if (before && after) {
			return;
		}
		if (token.text.startsWith("_") && !before && !after) {
			return;
		}

		const top = open[open.length - 1];
		if (top !== undefined && tokens[top].text === token.text) {
			open.pop();
			pairs.set(top, k);
			pairs.set(k, top);
		} else {
			open.push(k);
		}
	});

	return pairs;
}

// Runs of body cells, each widened until no marker in it has its partner
// outside, then merged where they overlap.
function widened(
	cells: Cell[],
	first: number,
	then: Token[],
	now: Token[],
): [number, number][] {
	const sides = [
		{ tokens: then, pairs: partners(then), of: (c: Cell) => c.then },
		{ tokens: now, pairs: partners(now), of: (c: Cell) => c.now },
	].map((side) => {
		const cellOf = new Map<number, number>();
		cells.forEach((cell, c) => {
			const [from, to] = side.of(cell);
			for (let k = from; k < to; k++) {
				cellOf.set(k, c);
			}
		});
		return { ...side, cellOf };
	});

	const runs: [number, number][] = [];
	for (let c = first; c < cells.length; c++) {
		if (cells[c].same) {
			continue;
		}
		let [a, b] = [c, c];
		let grown = true;
		while (grown) {
			grown = false;
			for (const side of sides) {
				const [from, to] = [side.of(cells[a])[0], side.of(cells[b])[1]];
				for (let k = from; k < to; k++) {
					const partner = side.pairs.get(k);
					if (partner === undefined) {
						continue;
					}
					const other = side.cellOf.get(partner)!;
					if (other < a || other > b) {
						[a, b] = [Math.min(a, other), Math.max(b, other)];
						grown = true;
					}
				}
			}
		}
		runs.push([a, b]);
	}

	// Nothing but space inside one paragraph between two runs, so they read as
	// one edit. Cells between runs are always unchanged, the same on both sides.
	const blank = (from: number, to: number) =>
		cells
			.slice(from, to)
			.every((cell) =>
				then
					.slice(cell.then[0], cell.then[1])
					.every(
						(token) =>
							WHITESPACE.test(token.text) &&
							!PARAGRAPH_BREAK.test(token.text),
					),
			);

	runs.sort((one, other) => one[0] - other[0]);
	const merged: [number, number][] = [];
	for (const run of runs) {
		const last = merged[merged.length - 1];
		if (
			last !== undefined &&
			(run[0] <= last[1] || blank(last[1] + 1, run[0]))
		) {
			last[1] = Math.max(last[1], run[1]);
		} else {
			merged.push(run);
		}
	}
	return merged;
}

// Where each token starts in its text, with the text's length at the end.
function offsets(tokens: Token[]): number[] {
	const starts = [0];
	for (const token of tokens) {
		starts.push(starts[starts.length - 1] + token.text.length);
	}
	return starts;
}

/** What differs from `then` to `now`, in the order the text reads. */
export function differences(then: string, now: string): Difference[] {
	const [old, current] = [tokens(then), tokens(now)];
	const [thenAt, nowAt] = [offsets(old), offsets(current)];
	const span = (at: number[], [from, to]: [number, number]): Span => ({
		from: at[from],
		to: at[to],
	});

	// A run that crosses from the front matter into the body is cut where the
	// front matter ends, so each part is told apart.
	const cells: Cell[] = [];
	for (const cell of align(old, current)) {
		if (cell.same) {
			cells.push(cell);
			continue;
		}
		const thenCut =
			cell.then[0] + where(old, cell.then, (t) => t.field).length;
		const nowCut =
			cell.now[0] + where(current, cell.now, (t) => t.field).length;
		const head: Cell = {
			then: [cell.then[0], thenCut],
			now: [cell.now[0], nowCut],
			same: false,
		};
		const tail: Cell = {
			then: [thenCut, cell.then[1]],
			now: [nowCut, cell.now[1]],
			same: false,
		};
		for (const part of [head, tail]) {
			if (part.then[0] < part.then[1] || part.now[0] < part.now[1]) {
				cells.push(part);
			}
		}
	}

	const isField = (cell: Cell) =>
		cell.then[0] < cell.then[1]
			? old[cell.then[0]].field
			: current[cell.now[0]].field;
	const firstBody = cells.findIndex((cell) => !isField(cell));
	const bodyFrom = firstBody === -1 ? cells.length : firstBody;

	const fields: Difference[] = cells
		.slice(0, bodyFrom)
		.filter((cell) => !cell.same)
		.flatMap((cell) => byField(cell, old, current))
		.map(({ key, then: t, now: n }) => ({
			kind: "field",
			key,
			then: span(thenAt, t),
			now: span(nowAt, n),
		}));

	const body: Difference[] = widened(cells, bodyFrom, old, current).map(
		([a, b]) => ({
			kind: "body",
			then: span(thenAt, [cells[a].then[0], cells[b].then[1]]),
			now: span(nowAt, [cells[a].now[0], cells[b].now[1]]),
		}),
	);

	return [...fields, ...body];
}

/** The text now, with one difference put back the way it was then. */
export function putBack(
	now: string,
	then: string,
	difference: Difference,
): string {
	return (
		now.slice(0, difference.now.from) +
		then.slice(difference.then.from, difference.then.to) +
		now.slice(difference.now.to)
	);
}
