// One side of the compare view: a document's body broken into the blocks the
// editor would draw, each a list of runs that know their emphasis and which
// difference, if any, washes them.

import type { Difference, Span } from "../differences";
import { split } from "../frontmatter";
import { BREAK_LINE } from "../markdown";
import { words } from "../words";

/** A stretch of text drawn one way. An empty run marks words the other side has. */
export type Run = {
	text: string;
	strong: boolean;
	em: boolean;
	difference: number | null;
};

export type Block =
	| { kind: "paragraph" | "quote" | "item"; runs: Run[] }
	| { kind: "heading"; level: number; runs: Run[] }
	| { kind: "break" };

type Range = { from: number; to: number };

const HEADING = /^(#{1,6})\s+/;
const QUOTE = /^>\s?/;
const ITEM = /^\s*(?:[-*+]|\d+\.)\s+/;
const MARKER = /\*\*|__|\*|_/g;

/**
 * The body of `text` as blocks, washed by `spans`, which are indexed like the
 * differences they come from. Offsets are into the whole text, front matter
 * and all, as `differences` gives them.
 */
export function blocks(text: string, spans: Span[]): Block[] {
	const { body } = split(text);
	const found: Block[] = [];
	// Each difference with no words on this side is drawn once, wherever it
	// first falls.
	const placed = new Set<number>();
	let lines: Range[] = [];
	let open: "paragraph" | "quote" = "paragraph";

	function flush() {
		if (lines.length > 0) {
			found.push({ kind: open, runs: runs(text, lines, spans, placed) });
		}
		lines = [];
	}

	let at = text.length - body.length;
	for (const line of body.split("\n")) {
		const from = at;
		const to = at + line.length;
		at = to + 1;
		const heading = HEADING.exec(line);
		const quote = QUOTE.exec(line);
		const item = ITEM.exec(line);

		if (line.trim() === "") {
			flush();
		} else if (BREAK_LINE.test(line.trim())) {
			flush();
			found.push({ kind: "break" });
		} else if (heading !== null) {
			flush();
			const content = { from: from + heading[0].length, to };
			found.push({
				kind: "heading",
				level: heading[1].length,
				runs: runs(text, [content], spans, placed),
			});
		} else if (item !== null) {
			flush();
			const content = { from: from + item[0].length, to };
			found.push({
				kind: "item",
				runs: runs(text, [content], spans, placed),
			});
		} else {
			const kind = quote === null ? "paragraph" : "quote";
			if (kind !== open) {
				flush();
				open = kind;
			}
			lines.push({ from: from + (quote?.[0].length ?? 0), to });
		}
	}
	flush();
	return found;
}

// ponytail: every cut checks every span, so a long text with thousands of
// differences is quadratic; index the spans by offset if that ever shows.
function runs(
	text: string,
	ranges: Range[],
	spans: Span[],
	placed: Set<number>,
): Run[] {
	const found: Run[] = [];
	let strong = false;
	let em = false;

	function covering(at: number): number | null {
		const index = spans.findIndex(
			(span) => span.from <= at && at < span.to,
		);
		return index === -1 ? null : index;
	}

	function pieces(from: number, to: number) {
		const inside = spans
			.flatMap((span) => [span.from, span.to])
			.filter((cut) => cut > from && cut < to);
		const cuts = [...new Set([from, to, ...inside])].sort((a, b) => a - b);

		cuts.forEach((cut, index) => {
			spans.forEach((span, difference) => {
				if (
					span.from === cut &&
					span.to === cut &&
					!placed.has(difference)
				) {
					placed.add(difference);
					found.push({ text: "", strong, em, difference });
				}
			});
			const next = cuts[index + 1];
			if (next !== undefined) {
				found.push({
					text: text.slice(cut, next),
					strong,
					em,
					difference: covering(cut),
				});
			}
		});
	}

	ranges.forEach((range, index) => {
		if (index > 0) {
			found.push({
				text: "\n",
				strong,
				em,
				difference: covering(range.from - 1),
			});
		}
		let last = range.from;
		const line = text.slice(range.from, range.to);
		for (const marker of line.matchAll(MARKER)) {
			const start = range.from + (marker.index ?? 0);
			pieces(last, start);
			if (marker[0].length === 2) {
				strong = !strong;
			} else {
				em = !em;
			}
			last = start + marker[0].length;
		}
		pieces(last, range.to);
	});
	return found.filter((run) => run.text !== "" || run.difference !== null);
}

/** Words the body differences take out of `then` and put into `now`. */
export function tally(
	then: string,
	now: string,
	found: Difference[],
): { added: number; removed: number } {
	let added = 0;
	let removed = 0;
	for (const difference of found) {
		if (difference.kind === "body") {
			removed += words(
				then.slice(difference.then.from, difference.then.to),
			);
			added += words(now.slice(difference.now.from, difference.now.to));
		}
	}
	return { added, removed };
}
