// The things every row in the sidebar needs that are decisions rather than
// shape: how far in it sits, whether it is the row being retitled, and what it
// offers a drag.

import type { CSSProperties } from "react";
import type { Spot } from "./reorder";
import type { Row } from "./tree";

/** How far in a row sits, as the stylesheet reads it. */
export function indent(depth: number) {
	return { "--depth": depth } as CSSProperties;
}

/**
 * The row the writer is retitling, and what Rust made of the last name they
 * tried. At most one row is being retitled at a time, so this holds one id
 * rather than sitting on every row. Whether that id is a document or a folder
 * rides with the row that opened the field, since only that row draws it.
 */
export type Renaming =
	| { kind: "closed" }
	| { kind: "open"; id: string }
	| { kind: "saving"; id: string }
	| { kind: "refused"; id: string; message: string };

/** What the field on a row needs in order to draw itself. */
export type Retitling = { busy: boolean; error: string | null };

/**
 * The state of the field on the row with this id, or null for every other row,
 * which is all of them but one.
 */
export function retitling(renaming: Renaming, id: string): Retitling | null {
	if (renaming.kind === "closed" || renaming.id !== id) {
		return null;
	}

	return {
		busy: renaming.kind === "saving",
		error: renaming.kind === "refused" ? renaming.message : null,
	};
}

/**
 * What one row offers a drag. The section and the rows drawn inside whatever is
 * being dragged are the same for every row in the list, so they are handed in
 * here rather than worked out at each row.
 */
export function spot(
	row: Row,
	section: string,
	held: ReadonlySet<string>,
): Spot {
	const id = row.kind === "folder" ? row.id : row.document.id;

	return {
		id,
		at: row.index,
		group: row.group,
		section,
		kind: row.kind === "folder" ? row.folderKind : null,
		folder: row.kind === "folder",
		holds: row.kind === "folder" ? row.holds : 0,
		within: held.has(id),
	};
}
