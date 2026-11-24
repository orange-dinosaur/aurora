// A screenplay's body is Fountain text. This reads it into the blocks the
// editor shows, one block to each non-blank line, and writes blocks back. Rust
// holds the same reading in `script::fountain`, and both are held to
// `fountain.fixture.json`.

export type BlockKind =
	| "heading"
	| "action"
	| "character"
	| "dialogue"
	| "parenthetical"
	| "transition"
	| "centered"
	| "page-break";

export type Block = {
	kind: BlockKind;
	/** The line without its forcing mark. */
	text: string;
	/** Whether a mark in the text decided the kind rather than Fountain's guess. */
	forced: boolean;
};

const HEADING = /^(int\.?\/ext|int|ext|est|i\/e)[. ]/i;
const CENTERED = /^>\s*(.*?)\s*<$/;
const PAGE_BREAK = /^={3,}$/;

/** Every letter in capitals, and at least one letter. An extension such as
 * "(cont'd)" may be in any case, so only what comes before it counts. */
function shouting(line: string): boolean {
	const name = line.split("(")[0];
	return /\p{L}/u.test(name) && name === name.toUpperCase();
}

function parenthetical(line: string): boolean {
	return /^\(.*\)$/.test(line.trim());
}

/**
 * Reads a body into blocks. Each line is judged with the lines around it, as
 * Fountain does: a heading and a transition stand between blank lines, a cue
 * is a capitalised line with text straight under it, and what follows a cue up
 * to the next blank line is its dialogue.
 */
export function parse(body: string): Block[] {
	const lines = body.split(/\r?\n/);
	const blank = (i: number) =>
		i < 0 || i >= lines.length || lines[i].trim() === "";
	const blocks: Block[] = [];
	let speaking = false;

	lines.forEach((line, i) => {
		const trimmed = line.trim();
		if (trimmed === "") {
			speaking = false;
			return;
		}
		const block = (kind: BlockKind, text = line, forced = false) =>
			blocks.push({ kind, text, forced });

		if (speaking) {
			block(parenthetical(trimmed) ? "parenthetical" : "dialogue");
			return;
		}

		const centered = CENTERED.exec(trimmed);
		if (PAGE_BREAK.test(trimmed)) {
			block("page-break", "");
		} else if (centered) {
			block("centered", centered[1]);
		} else if (trimmed.startsWith("!")) {
			block("action", line.slice(line.indexOf("!") + 1), true);
		} else if (trimmed.startsWith("@")) {
			block("character", line.slice(line.indexOf("@") + 1), true);
			speaking = true;
		} else if (trimmed.startsWith(">")) {
			block("transition", line.slice(line.indexOf(">") + 1), true);
		} else if (/^\.[^.]/.test(trimmed)) {
			block("heading", line.slice(line.indexOf(".") + 1), true);
		} else if (blank(i - 1) && HEADING.test(trimmed)) {
			block("heading");
		} else if (
			blank(i - 1) &&
			blank(i + 1) &&
			shouting(trimmed) &&
			trimmed.endsWith("TO:")
		) {
			block("transition");
		} else if (blank(i - 1) && !blank(i + 1) && shouting(trimmed)) {
			block("character");
			speaking = true;
		} else {
			block("action");
		}
	});

	return blocks;
}

/**
 * The kinds a run of editor blocks should have now their text has changed. The
 * editor has no blank lines, so the kind a block already has stands in for
 * one: the line under a cue or a parenthetical is speech, the line under
 * dialogue is speech only if it already was, and a
 * cue stays a cue whatever follows it. A capitalised line becomes a cue only
 * at the end, above an empty block or above speech. A block forced by a mark
 * or by the writer, and the kinds only a mark makes, keep their kind.
 */
export function classify(blocks: Block[]): BlockKind[] {
	const kinds: BlockKind[] = [];
	blocks.forEach((block, i) => {
		const before = i > 0 ? kinds[i - 1] : null;
		const next = blocks[i + 1];
		if (
			block.forced ||
			block.kind === "centered" ||
			block.kind === "page-break" ||
			block.text.trim() === ""
		) {
			kinds.push(block.kind);
			return;
		}
		const speaking =
			before === "character" ||
			before === "parenthetical" ||
			(before === "dialogue" && SPOKEN.has(block.kind));
		const heard =
			block.kind === "character" ||
			next === undefined ||
			next.text.trim() === "" ||
			SPOKEN.has(next.kind);
		const text = [speaking ? "@A" : "", block.text, heard ? "A" : ""];
		kinds.push(parse(text.join("\n"))[speaking ? 1 : 0].kind);
	});
	return kinds;
}

/** The block Enter starts after a block of this kind. */
export function follows(kind: BlockKind): BlockKind {
	return kind === "character" || kind === "parenthetical"
		? "dialogue"
		: kind === "transition"
			? "heading"
			: "action";
}

/** The blocks Tab steps through, in order. */
export const CYCLE: BlockKind[] = [
	"heading",
	"action",
	"character",
	"dialogue",
	"parenthetical",
	"transition",
];

/**
 * The kind Tab gives a block, or Shift+Tab when `step` is -1, so Shift+Tab
 * always undoes Tab. Centered text and a page break sit outside the cycle and
 * enter it at action.
 */
export function cycled(kind: BlockKind, step: 1 | -1): BlockKind {
	const at = CYCLE.indexOf(kind);
	if (at === -1) {
		return "action";
	}
	return CYCLE[(at + step + CYCLE.length) % CYCLE.length];
}

/** A character's name as their cue. */
export function cueFor(name: string): string {
	return name.toUpperCase();
}

/** Whether a cue names this character: the cue, without extensions such as
 * "(V.O.)" and without a dual-dialogue caret, is the name in capitals. */
export function cueNames(cue: string, name: string): boolean {
	const bare = bareCue(cue).toUpperCase();
	return bare !== "" && bare === cueFor(name.trim());
}

function bareCue(cue: string): string {
	return cue
		.replace(/\(.*?\)/g, "")
		.replace(/\^\s*$/, "")
		.trim();
}

/** Each word starting with a capital and the rest in lower case. */
function titled(text: string): string {
	return text
		.toLowerCase()
		.replace(
			/(^|[\s-])(\p{L})/gu,
			(_, before, letter) => `${before}${letter.toUpperCase()}`,
		);
}

/** The name a cue gives a character who has no page yet: MARCUS is Marcus. */
export function cueTitle(cue: string): string {
	return titled(bareCue(cue));
}

/** The place a scene heading names, as a page title: `INT. KITCHEN - NIGHT`
 * is Kitchen. The time after the last dash is left out; a heading with no
 * place gives an empty string. */
export function headingPlace(heading: string): string {
	const place = heading
		.replace(/#[^#]*#\s*$/, "")
		.replace(HEADING, "")
		.replace(/^\./, "")
		.replace(/\s+-\s+[^-]*$/, "")
		.trim();
	return titled(place);
}

/** The start of a scene heading set at a place, left for the time of day. */
export function headingAt(place: string): string {
	return `INT. ${place.toUpperCase()} - `;
}

/** Whether giving block `at` this kind overrules what its text would make it.
 * A kind chosen for an empty line always holds, or the first capital typed into
 * a heading would turn it into a cue. */
export function overrules(
	blocks: Block[],
	at: number,
	kind: BlockKind,
): boolean {
	if (blocks[at].text.trim() === "") {
		return true;
	}
	const chosen = blocks.map((block, i) =>
		i === at ? { ...block, kind, forced: false } : block,
	);
	return classify(chosen)[at] !== kind;
}

export type Style = "bold" | "italic" | "underline";

/** A stretch of a line in one emphasis. */
export type Span = { text: string } & Record<Style, boolean>;

// Checked in this order, so `**` is read as bold before `*` can be italic.
const EMPHASIS: [string, Style][] = [
	["**", "bold"],
	["*", "italic"],
	["_", "underline"],
];
const MARKER: Record<Style, string> = {
	bold: "**",
	italic: "*",
	underline: "_",
};
const OPENING: Style[] = ["underline", "bold", "italic"];

/** Whether an unescaped `marker` comes at or after `from`. */
function closes(text: string, from: number, marker: string): boolean {
	for (let i = from; i < text.length; i++) {
		if (text[i] === "\\") {
			i++;
		} else if (text.startsWith(marker, i)) {
			return true;
		}
	}
	return false;
}

/**
 * Reads a line's emphasis: `**bold**`, `*italic*` and `_underline_`, nested
 * as `***both***`, with `\*`, `\_` and `\\` for the characters themselves. A
 * marker with nothing to close it is only a character.
 */
export function spans(text: string): Span[] {
	const found: Span[] = [];
	const on: Record<Style, boolean> = {
		bold: false,
		italic: false,
		underline: false,
	};
	let run = "";
	const flush = () => {
		if (run !== "") {
			found.push({ text: run, ...on });
			run = "";
		}
	};
	for (let i = 0; i < text.length;) {
		if (text[i] === "\\" && "*_\\".includes(text[i + 1] ?? "")) {
			run += text[i + 1];
			i += 2;
			continue;
		}
		const hit = EMPHASIS.find(
			([marker, style]) =>
				text.startsWith(marker, i) &&
				(on[style] || closes(text, i + marker.length, marker)),
		);
		if (hit === undefined) {
			run += text[i];
			i++;
			continue;
		}
		flush();
		on[hit[1]] = !on[hit[1]];
		i += hit[0].length;
	}
	flush();
	return found;
}

/** Writes spans back as one line, opening and closing only where emphasis changes. */
export function emphasised(line: Span[]): string {
	const open: Style[] = [];
	let text = "";
	for (const span of line) {
		if (span.text === "") {
			continue;
		}
		while (open.some((style) => !span[style])) {
			text += MARKER[open.pop() as Style];
		}
		for (const style of OPENING) {
			if (span[style] && !open.includes(style)) {
				text += MARKER[style];
				open.push(style);
			}
		}
		text += span.text
			.replace(/\\(?=[*_\\]|$)/g, "\\\\")
			.replace(/[*_]/g, "\\$&");
	}
	while (open.length > 0) {
		text += MARKER[open.pop() as Style];
	}
	return text;
}

// The kinds a mark can force, and the mark that does it.
const MARKS: Partial<Record<BlockKind, string>> = {
	heading: ".",
	character: "@",
	action: "!",
	transition: ">",
};

const SPOKEN: ReadonlySet<BlockKind> = new Set(["dialogue", "parenthetical"]);
const VOICED: ReadonlySet<BlockKind> = new Set([
	"character",
	"dialogue",
	"parenthetical",
]);

function written(blocks: Block[], marked: ReadonlySet<number>): string {
	return blocks
		.map((block, i) => {
			const line =
				block.kind === "page-break"
					? "==="
					: block.kind === "centered"
						? `> ${block.text} <`
						: marked.has(i)
							? `${MARKS[block.kind]}${block.text}`
							: block.text;
			const joined =
				i > 0 &&
				VOICED.has(blocks[i - 1].kind) &&
				SPOKEN.has(block.kind);
			return i === 0 ? line : joined ? `\n${line}` : `\n\n${line}`;
		})
		.join("");
}

/**
 * Writes blocks as Fountain, with a blank line between elements and none
 * inside a cue's dialogue. A forcing mark goes only on a block that Fountain
 * would otherwise read as something else, so a body reads as plain prose
 * wherever it can. Marking one block can change how the next is read, so this
 * writes, reads back and marks again until nothing more needs one.
 */
export function serialise(blocks: Block[]): string {
	const marked = new Set<number>();
	for (;;) {
		const text = written(blocks, marked);
		const read = parse(text);
		if (read.length !== blocks.length) {
			return text;
		}
		const more = blocks.flatMap((block, i) =>
			read[i].kind !== block.kind && MARKS[block.kind] && !marked.has(i)
				? [i]
				: [],
		);
		if (more.length === 0) {
			return text;
		}
		more.forEach((i) => marked.add(i));
	}
}
