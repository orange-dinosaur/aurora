// The outline is worked out from a copy of the document's top-level blocks,
// the same way find works from a copy of its runs of text: the column reads
// them out of the editor, this turns them into a list, and nothing here can
// touch a node.

/** A top-level block, as the outline sees it. */
export type Block = {
	key: string;
	/** 1 to 6 for a heading, null for anything that is not one. */
	level: number | null;
	text: string;
};

/** A heading in the list, with the indent it is drawn at. */
export type Entry = {
	key: string;
	level: number;
	text: string;
	/** Counted in steps of indentation, not in heading levels. */
	depth: number;
};

/**
 * The headings in reading order. Depth comes from the levels the document
 * actually uses rather than from the level itself, so a chapter written
 * entirely in Heading 2, or one that jumps from 1 straight to 3, still reads as
 * a list instead of as a ladder with rungs missing.
 */
export function outlined(blocks: Block[]): Entry[] {
	const entries: Entry[] = [];
	// The levels of the headings this one is still inside.
	const open: number[] = [];

	for (const block of blocks) {
		if (block.level === null) {
			continue;
		}

		// Anything at this level or below it is a sibling or an uncle, not a
		// parent, so it stops counting towards the indent.
		while (open.length > 0 && open[open.length - 1] >= block.level) {
			open.pop();
		}

		entries.push({
			key: block.key,
			level: block.level,
			text: block.text,
			depth: open.length,
		});
		open.push(block.level);
	}

	return entries;
}

/**
 * The heading whose section the given block sits in, which is what marks where
 * the writer is. Null while the caret is above the first heading, or when the
 * block is not one of these.
 */
export function containing(blocks: Block[], at: string | null): string | null {
	if (at === null) {
		return null;
	}

	let heading: string | null = null;
	for (const block of blocks) {
		if (block.level !== null) {
			heading = block.key;
		}
		if (block.key === at) {
			return heading;
		}
	}

	return null;
}
