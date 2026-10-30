// One side of the compare view: a document's body broken into the blocks the
// editor would draw, each a list of runs that know their emphasis and which
// difference, if any, washes them.

import type { TakeView } from "../bindings/TakeView";
import { type Difference, differences, type Span } from "../differences";
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

/**
 * One page of a document's `live` text with the words `prose` had and it lost
 * set in just before what replaced them. `cut` holds the differences that are
 * those words, and every other difference is words put in. Space at either end
 * of the prose is not a change, as a take's file keeps it differently.
 */
export function marked(
	live: string,
	prose: string,
): { body: Block[]; cut: Set<number> } {
	const { body } = split(live);
	const head = live.slice(0, live.length - body.length);
	const then = head + prose.trim();
	const now = head + body.trim();
	const spans: Span[] = [];
	const cut = new Set<number>();
	let page = "";
	let at = 0;

	function mark(words: string) {
		spans.push({ from: page.length, to: page.length + words.length });
		page += words;
	}

	for (const found of differences(then, now)) {
		page += now.slice(at, found.now.from);
		at = found.now.to;
		// ponytail: cut words lose their emphasis, as a lone marker would flip
		// the words after it. Pair the markers up if the plain cuts jar.
		const gone = then
			.slice(found.then.from, found.then.to)
			.replace(MARKER, "");
		const put = now.slice(found.now.from, found.now.to);
		if (gone.trim() !== "") {
			cut.add(spans.length);
			mark(gone);
			if (put !== "" && !/\s$/.test(gone)) {
				page += " ";
			}
		}
		if (put !== "") {
			mark(put);
		}
	}
	return { body: blocks(page + now.slice(at), spans), cut };
}

/** The take to measure the chosen one against: its start, or the one to its left. */
export function baseline(takes: TakeView[]): string | null {
	const at = takes.findIndex((take) => take.chosen);
	if (at === -1) {
		return null;
	}
	const start = takes.find((take) => take.id === takes[at].from);
	return (start ?? takes[at - 1])?.id ?? null;
}

/** Whether a difference only puts words in, only takes them out, or swaps them. */
export function change(
	difference: Difference,
): "added" | "removed" | "swapped" {
	if (difference.then.from === difference.then.to) {
		return "added";
	}
	return difference.now.from === difference.now.to ? "removed" : "swapped";
}

/**
 * Where height `y` in one column falls in the other, given pairs of heights
 * that line up, from the top down. Between two pairs it moves in proportion,
 * so a passage only one side has holds the other still. A pair that would run
 * backwards, as two differences on one line can, is skipped.
 */
export function follow(pairs: [number, number][], y: number): number {
	let last: [number, number] = [0, 0];
	for (const pair of pairs) {
		if (pair[0] < last[0] || pair[1] < last[1]) {
			continue;
		}
		if (y <= pair[0]) {
			const span = pair[0] - last[0];
			return span === 0
				? pair[1]
				: last[1] + ((y - last[0]) / span) * (pair[1] - last[1]);
		}
		last = pair;
	}
	return last[1] + (y - last[0]);
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
