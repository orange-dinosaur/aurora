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
