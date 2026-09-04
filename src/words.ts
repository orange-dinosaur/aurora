// Rust counts a document's words with `text.split_whitespace().count()` when
// it summarises a file for a section card. These are the same counts for the
// editor, so the two can never disagree about the document on screen.

import { split } from "./frontmatter";

/**
 * A document's own writing: everything but the front matter, which is data
 * about the page rather than words in it. Rust counts a file this way too,
 * through `body`, so both ends agree about how long a chapter is and naming a
 * field is not the same as writing a sentence.
 */
export function prose(text: string): string {
	return split(text).body;
}

// `split_whitespace` breaks on Unicode's White_Space property, which is not
// quite JavaScript's `\s`: it takes NEL and leaves out the byte-order mark.
// Spelling the code points out keeps the two counts provably the same.
function separates(code: number): boolean {
	return (
		(code >= 0x09 && code <= 0x0d) || // tab, newline, vertical tab, form feed, carriage return
		code === 0x20 || // space
		code === 0x85 || // next line
		code === 0xa0 || // no-break space
		code === 0x1680 || // ogham space mark
		(code >= 0x2000 && code <= 0x200a) || // en quad through hair space
		code === 0x2028 || // line separator
		code === 0x2029 || // paragraph separator
		code === 0x202f || // narrow no-break space
		code === 0x205f || // medium mathematical space
		code === 0x3000 // ideographic space
	);
}

/** How many words the writer has, counted the way the Rust side counts them. */
export function words(text: string): number {
	let count = 0;
	let inside = false;

	for (let at = 0; at < text.length; at += 1) {
		if (separates(text.charCodeAt(at))) {
			inside = false;
		} else if (!inside) {
			inside = true;
			count += 1;
		}
	}

	return count;
}

// Walks the text a character at a time and counts the ones that answer. The
// step is by code point rather than by UTF-16 unit, so an emoji or a letter
// outside the basic plane arrives whole and counts once.
function walk(text: string, wanted: (code: number) => boolean): number {
	let count = 0;
	let at = 0;

	while (at < text.length) {
		const code = text.codePointAt(at) ?? 0;
		at += code > 0xffff ? 2 : 1;
		if (wanted(code)) {
			count += 1;
		}
	}

	return count;
}

/** Every character, spaces and newlines included. */
export function characters(text: string): number {
	return walk(text, () => true);
}

/** The same, with every space, tab and newline taken out. */
export function charactersWithoutSpaces(text: string): number {
	return walk(text, (code) => !separates(code));
}
