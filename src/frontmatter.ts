// The fields Aurora keeps in a document's front matter. `markdown.ts` lifts
// the block off the top of a file and puts it back on save without looking
// inside it; this is the only place that reads it as data.
//
// What it understands is a deliberate subset of YAML: a key at the left margin
// holding one line of text, a list of them, or a block of name-to-line pairs
// one level in. Everything else, from a comment to a blank line to a map nested
// two deep, is kept exactly as the writer left it and handed back untouched. A
// block Aurora did not write therefore comes out of a save reformatted only
// where a field actually changed.

/** A field that pairs names with a line each, which is what `relationships` is. */
export type Ties = Map<string, string>;

/**
 * What one field holds: a line of text, a list of them, named lines, or a
 * plain yes-or-no. Only Aurora's own switches put a boolean here; a `false` a
 * writer typed themselves stays the text they typed, so their file is never
 * reinterpreted behind them.
 */
export type Value = string | string[] | Ties | boolean;

/**
 * The block some note-taking apps fence off at the top of a file. Its fence is
 * spelled the same as a scene break, so it is lifted off before anything else
 * looks at the text. Rust matches the same shape in `document::body`, which is
 * how the word count and the card excerpt skip it.
 */
const FRONT_MATTER = /^---\n[\s\S]*?\n---[ \t]*\n?/;

/**
 * The key a scene carries when the writer has taken it out of the book. It is
 * written only when it is `false`, so a document that says nothing is in: a
 * project written before the switch existed exports whole.
 */
export const IN_BOOK = "inBook";

/** The fields Aurora has controls of its own for. The rest are the writer's. */
export const BUILT_IN = [
	"names",
	"tags",
	"synopsis",
	"remarks",
	"relationships",
	IN_BOOK,
];

/** A document's fields, in the order its file lists them. */
export type Fields = Map<string, Value>;

/** A key at the left margin, and whatever follows the colon. */
const KEY = /^([A-Za-z0-9_][^:\n]*):(.*)$/;

/** One item of a block list, indented under its key. */
const ITEM = /^\s*-\s*(.*?)\s*$/;

/** An item that is really a map of its own, which we will not touch. */
const NESTED = /:(?:\s|$)/;

/**
 * A `name: line` pair indented under a key, which is how a tie is written. The
 * indent is captured because every pair in a block has to share it: a line that
 * steps in again is a map deeper than Aurora reads.
 */
const PAIR = /^(\s+)([A-Za-z0-9_"'][^:\n]*):(.*)$/;

/** What a plain scalar may not start with, since YAML gives it a meaning. */
const OPAQUE = /^[#&*!|>{}%@`]/;

/** Words YAML reads as something other than text. */
const RESERVED = /^(?:true|false|yes|no|on|off|null|~)$/i;

const NUMERIC = /^[-+]?(?:\d[\d_]*(?:\.\d*)?|\.\d+)(?:[eE][-+]?\d+)?$/;

/**
 * One run of lines: either a field Aurora understands, or a stretch it keeps
 * verbatim.
 */
type Segment =
	| { key: string; value: Value; text: string[] }
	| { key: null; value: null; text: string[] };

/** The block's lines, with its fences and any trailing newline removed. */
function lines(block: string): string[] {
	const found = block.replace(/\r\n/g, "\n").split("\n");

	if (found[0]?.trim() === "---") {
		found.shift();
	}
	if (found.length > 0 && found[found.length - 1].trim() === "") {
		found.pop();
	}
	if (found.length > 0 && found[found.length - 1].trim() === "---") {
		found.pop();
	}

	return found;
}

/** A scalar or a flow list as written, or null if it is not either. */
function read(text: string): Value | null {
	if (text.startsWith("[") && text.endsWith("]")) {
		const inside = text.slice(1, -1).trim();
		if (inside === "") {
			return [];
		}

		const items = divide(inside);
		if (items === null) {
			return null;
		}

		const values = items.map((item) => unquote(item.trim()));
		return values.every((value) => value !== null)
			? (values as string[])
			: null;
	}

	return OPAQUE.test(text) ? null : unquote(text);
}

/** A flow list's items, split on the commas that are not inside a quote. */
function divide(text: string): string[] | null {
	const items: string[] = [];
	let item = "";
	let quote = "";

	for (const char of text) {
		if (quote !== "") {
			item += char;
			if (char === quote) {
				quote = "";
			}
		} else if (char === '"' || char === "'") {
			quote = char;
			item += char;
		} else if (char === ",") {
			items.push(item);
			item = "";
		} else {
			item += char;
		}
	}

	if (quote !== "") {
		return null;
	}

	items.push(item);
	return items;
}

/** The text a scalar stands for, or null if it stands for nothing usable. */
function unquote(text: string): string | null {
	if (text.startsWith('"')) {
		if (text.length < 2 || !text.endsWith('"')) {
			return null;
		}
		return text
			.slice(1, -1)
			.replace(/\\(.)/g, (_, char: string) =>
				char === "n" ? "\n" : char === "t" ? "\t" : char,
			);
	}

	if (text.startsWith("'")) {
		if (text.length < 2 || !text.endsWith("'")) {
			return null;
		}
		return text.slice(1, -1).replace(/''/g, "'");
	}

	// A plain scalar ends where a comment begins.
	const plain = text.replace(/\s+#.*$/, "").trim();
	return plain === "" ? null : plain;
}

/** A value as one line of YAML, quoted only when it has to be. */
function quote(value: string): string {
	const plain =
		value !== "" &&
		value === value.trim() &&
		!OPAQUE.test(value) &&
		!RESERVED.test(value) &&
		!NUMERIC.test(value) &&
		!/^[-?:,[\]'"]/.test(value) &&
		!/[:#\n\t]/.test(value);

	if (plain) {
		return value;
	}

	const escaped = value
		.replace(/\\/g, "\\\\")
		.replace(/"/g, '\\"')
		.replace(/\n/g, "\\n")
		.replace(/\t/g, "\\t");

	return `"${escaped}"`;
}

/** One field written the way Aurora writes it. */
function emit(key: string, value: Value): string[] {
	if (typeof value === "string") {
		return [`${key}: ${quote(value)}`];
	}
	// Bare, because a quoted "false" is the word rather than the answer.
	if (typeof value === "boolean") {
		return [`${key}: ${value ? "true" : "false"}`];
	}
	if (value instanceof Map) {
		if (value.size === 0) {
			return [`${key}: {}`];
		}

		return [
			`${key}:`,
			...[...value].map(
				([name, note]) => `  ${quote(name)}: ${quote(note)}`,
			),
		];
	}
	if (value.length === 0) {
		return [`${key}: []`];
	}

	return [`${key}:`, ...value.map((item) => `  - ${quote(item)}`)];
}

function same(one: Value, other: Value): boolean {
	if (one instanceof Map || other instanceof Map) {
		if (!(one instanceof Map) || !(other instanceof Map)) {
			return false;
		}

		// Order counts: the file's own order is what gets kept when nothing
		// has changed, so a reordering has to read as a change.
		const mine = [...one];
		const theirs = [...other];
		return (
			mine.length === theirs.length &&
			mine.every(
				([name, note], at) =>
					theirs[at][0] === name && theirs[at][1] === note,
			)
		);
	}
	if (
		typeof one === "string" ||
		typeof other === "string" ||
		typeof one === "boolean" ||
		typeof other === "boolean"
	) {
		return one === other;
	}

	return (
		one.length === other.length &&
		one.every((item, at) => item === other[at])
	);
}

/** The block broken into the fields it sets and the lines it keeps. */
function segments(block: string): Segment[] {
	const source = lines(block);
	const found: Segment[] = [];
	let at = 0;

	while (at < source.length) {
		const key = KEY.exec(source[at]);
		if (key === null) {
			found.push({ key: null, value: null, text: [source[at]] });
			at += 1;
			continue;
		}

		const rest = key[2].trim();
		if (rest !== "") {
			const value = read(rest);
			found.push(
				value === null
					? { key: null, value: null, text: [source[at]] }
					: { key: key[1], value, text: [source[at]] },
			);
			at += 1;
			continue;
		}

		// A key on its own line heads a list or a block of pairs, so long as
		// every line under it is the same one of those. A block that mixes
		// them, or holds anything else, is a shape we leave alone.
		const items: string[] = [];
		const pairs: Ties = new Map();
		let shape: "list" | "pairs" | null = null;
		let indent: string | null = null;
		let end = at + 1;
		let usable = true;

		while (end < source.length) {
			const item = shape === "pairs" ? null : ITEM.exec(source[end]);
			const pair = shape === "list" ? null : PAIR.exec(source[end]);

			if (item !== null) {
				shape = "list";
				const value = NESTED.test(item[1]) ? null : unquote(item[1]);
				if (value === null) {
					usable = false;
				} else {
					items.push(value);
				}
			} else if (pair !== null) {
				shape = "pairs";
				indent ??= pair[1];
				const name = unquote(pair[2].trim());
				const rest = pair[3].trim();
				// A name with nothing after it is a tie the writer has not
				// worded yet, unless a deeper line follows, which makes it the
				// head of a map this does not read.
				const value = rest === "" ? "" : read(rest);
				if (
					pair[1] !== indent ||
					name === null ||
					typeof value !== "string"
				) {
					usable = false;
				} else {
					pairs.set(name, value);
				}
			} else {
				break;
			}
			end += 1;
		}

		const text = source.slice(at, end);
		let held: Value | null = null;
		if (usable && shape === "list" && items.length > 0) {
			held = items;
		} else if (usable && shape === "pairs" && pairs.size > 0) {
			held = pairs;
		}

		found.push(
			held === null
				? { key: null, value: null, text }
				: { key: key[1], value: held, text },
		);
		at = end;
	}

	return found;
}

/**
 * A field as one line of text, whatever shape the file gave it. A writer who
 * wrote a list where Aurora expects text sees it joined rather than nothing.
 */
export function text(fields: Fields, key: string): string {
	const value = fields.get(key);

	if (value === undefined) {
		return "";
	}
	if (typeof value === "string") {
		return value;
	}
	if (typeof value === "boolean") {
		return value ? "true" : "false";
	}

	return list(fields, key).join(", ");
}

/** A field as a list, whatever shape the file gave it. */
export function list(fields: Fields, key: string): string[] {
	const value = fields.get(key);

	if (value === undefined) {
		return [];
	}
	if (typeof value === "string") {
		return value === "" ? [] : [value];
	}
	if (typeof value === "boolean") {
		return [value ? "true" : "false"];
	}
	if (value instanceof Map) {
		return [...value].map(([name, note]) =>
			note === "" ? name : `${name}: ${note}`,
		);
	}

	return value;
}

/**
 * A field as the pairs it names, whatever shape the file gave it. A writer who
 * wrote a list where Aurora expects pairs sees each entry as a name with
 * nothing said about it yet, rather than seeing nothing at all.
 */
export function ties(fields: Fields, key: string): Ties {
	const value = fields.get(key);

	if (value instanceof Map) {
		return value;
	}

	return new Map(list(fields, key).map((item) => [item, ""]));
}

/** What YAML reads as no, whichever way the writer spelled it. */
const DENIED = /^(?:false|no|off)$/i;

/**
 * A yes-or-no field, which is yes unless the file says otherwise. A key that
 * is not there is an answer nobody has given, and Aurora's switches are all
 * on by default, so only a plain no takes one off.
 */
export function flag(fields: Fields, key: string): boolean {
	const value = fields.get(key);

	if (typeof value === "boolean") {
		return value;
	}
	if (typeof value === "string") {
		return !DENIED.test(value.trim());
	}

	return true;
}

/** A file in two pieces: the block at its top, fences and all, and the prose. */
export function split(text: string): { block: string; body: string } {
	const found = FRONT_MATTER.exec(text);

	return found === null
		? { block: "", body: text }
		: { block: found[0].trimEnd(), body: text.slice(found[0].length) };
}

/** The keys of the fields the writer added themselves, in the file's order. */
export function custom(fields: Fields): string[] {
	return [...fields.keys()].filter((key) => !BUILT_IN.includes(key));
}

/**
 * Every field name used across a project's files, for the Add field box to
 * suggest, so the same field ends up spelled the same way on every page.
 */
export function fieldNames(texts: string[]): string[] {
	const names: string[] = [];

	for (const file of texts) {
		for (const key of custom(parse(split(file).block))) {
			if (!names.includes(key)) {
				names.push(key);
			}
		}
	}

	return names.sort((one, other) => one.localeCompare(other));
}

/** The fields a front matter block sets, fences and all. */
export function parse(block: string): Fields {
	const fields: Fields = new Map();

	for (const segment of segments(block)) {
		if (segment.key !== null) {
			fields.set(segment.key, segment.value);
		}
	}

	return fields;
}

/**
 * The block to write back, given the fields now and the block they came from.
 * A field that has not changed keeps the spelling the writer gave it, a field
 * that has is rewritten in place, and a field no longer here is dropped. New
 * fields go at the end. Everything Aurora did not understand stays where it
 * was.
 */
export function serialize(fields: Fields, previous = ""): string {
	const written = new Set<string>();
	const out: string[] = [];

	for (const segment of segments(previous)) {
		if (segment.key === null) {
			out.push(...segment.text);
			continue;
		}
		if (written.has(segment.key)) {
			continue;
		}

		const value = fields.get(segment.key);
		if (value === undefined) {
			continue;
		}

		written.add(segment.key);
		out.push(
			...(same(value, segment.value)
				? segment.text
				: emit(segment.key, value)),
		);
	}

	for (const [key, value] of fields) {
		if (!written.has(key)) {
			out.push(...emit(key, value));
		}
	}

	return out.length === 0 ? "" : ["---", ...out, "---"].join("\n");
}
