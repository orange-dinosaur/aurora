// The fields Aurora keeps in a document's front matter. `markdown.ts` lifts
// the block off the top of a file and puts it back on save without looking
// inside it; this is the only place that reads it as data.
//
// What it understands is a deliberate subset of YAML: a key at the left margin
// holding one line of text, or a list of them. Everything else, from a nested
// map to a comment to a blank line, is kept exactly as the writer left it and
// handed back untouched. A block Aurora did not write therefore comes out of a
// save reformatted only where a field actually changed.

/** What one field holds: a line of text, or a list of them. */
export type Value = string | string[];

/** A document's fields, in the order its file lists them. */
export type Fields = Map<string, Value>;

/** A key at the left margin, and whatever follows the colon. */
const KEY = /^([A-Za-z0-9_][^:\n]*):(.*)$/;

/** One item of a block list, indented under its key. */
const ITEM = /^\s*-\s*(.*?)\s*$/;

/** An item that is really a map of its own, which we will not touch. */
const NESTED = /:(?:\s|$)/;

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
	if (value.length === 0) {
		return [`${key}: []`];
	}

	return [`${key}:`, ...value.map((item) => `  - ${quote(item)}`)];
}

function same(one: Value, other: Value): boolean {
	if (typeof one === "string" || typeof other === "string") {
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

		// A key on its own line heads a list, so long as every line under it
		// is an item of one. Anything else below it is a shape we leave alone.
		const items: string[] = [];
		let end = at + 1;
		let usable = true;

		while (end < source.length) {
			const item = ITEM.exec(source[end]);
			if (item === null) {
				break;
			}

			const value = NESTED.test(item[1]) ? null : unquote(item[1]);
			if (value === null) {
				usable = false;
			} else {
				items.push(value);
			}
			end += 1;
		}

		const text = source.slice(at, end);
		found.push(
			usable && items.length > 0
				? { key: key[1], value: items, text }
				: { key: null, value: null, text },
		);
		at = end;
	}

	return found;
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
