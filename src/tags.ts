// Reading and rewriting the tags in a document's front matter, from outside the
// document. `frontmatter.ts` knows what a tag is; this knows what happens to one
// when the page it points at is renamed.

import { list, parse, serialize, split } from "./frontmatter";

/**
 * Whether a document carries this tag. Case-sensitive, like recognition in
 * prose: `elena` and `Elena` are two tags, and only one of them means the page.
 */
export function tagged(text: string, tag: string): boolean {
	return list(parse(split(text).block), "tags").includes(tag);
}

/**
 * The same document with one tag rewritten, and everything else — the body, the
 * other fields, whatever Aurora does not understand — left exactly as it was. A
 * document that does not carry the tag comes back untouched, character for
 * character, so nothing is written for the sake of it.
 */
export function retag(text: string, from: string, to: string): string {
	const { block, body } = split(text);
	const fields = parse(block);
	const tags = list(fields, "tags");

	if (!tags.includes(from)) {
		return text;
	}

	// A document already wearing both ends up with one of them: renaming
	// `Elena` to `Wren` on a page tagged with each is not a page tagged twice.
	const next: string[] = [];
	for (const tag of tags) {
		const swapped = tag === from ? to : tag;
		if (!next.includes(swapped)) {
			next.push(swapped);
		}
	}

	const rewritten = new Map(fields);
	rewritten.set("tags", next);

	// `split` ate the newline under the block, so it goes back on.
	return `${serialize(rewritten, block)}\n${body}`;
}
