// The project's tree as a list the sidebar can draw. A nested list would mean
// a component that calls itself; one flat list of rows, each carrying how deep
// it sits, keeps the drawing in one place and the walking here, where it can
// be tested without a screen.

import type { FolderKind, ProjectDocument, TreeNode } from "./types";
import { MANUSCRIPT, mayHold } from "./kinds";

/**
 * Enough about a folder to open it and to know what may be made inside it. A
 * tab holds one of these, since the overview it opens has no other way to
 * learn what it is looking at.
 */
export type FolderRef = {
	id: string;
	name: string;
	kind: FolderKind | null;
	/** The top-level folder it lives under, which decides the kind rules. */
	section: string;
};

// Where a row sits, which is everything the sidebar needs that the node itself
// does not say.
type Place = {
	/** How many folders down from the section this row is. */
	depth: number;
	/**
	 * Its position among the nodes beside it. That is the index
	 * `reorder_document` takes, and the number the sidebar shows.
	 */
	index: number;
	/** How many nodes share its level, so the menu knows where it can go. */
	siblings: number;
	/**
	 * The id of the folder holding it. A node only ever moves within its own,
	 * so a drag that leaves the group is refused rather than quietly meaning
	 * something the writer did not ask for.
	 */
	group: string;
};

export type Row =
	| (Place & {
			kind: "folder";
			id: string;
			name: string;
			folderKind: FolderKind | null;
	  })
	| (Place & { kind: "document"; document: ProjectDocument });

/**
 * Everything under one folder, depth first: a folder, then all it holds, then
 * whatever follows it. The section itself is not a row; the sidebar draws its
 * own header.
 */
export function rows(nodes: TreeNode[], group: string, depth = 0): Row[] {
	return nodes.flatMap((node, index) => {
		const place = { depth, index, siblings: nodes.length, group };

		if (node.node === "document") {
			const document: ProjectDocument = {
				id: node.id,
				path: node.path,
				folder: node.folder,
				title: node.title,
				target: node.target,
			};
			const row: Row = { kind: "document", ...place, document };
			return [row];
		}

		const row: Row = {
			kind: "folder",
			...place,
			id: node.id,
			name: node.name,
			folderKind: node.kind,
		};
		return [row, ...rows(node.children, node.id, depth + 1)];
	});
}

/** How many documents a level holds, however deep they sit. */
export function documentsIn(nodes: TreeNode[]): number {
	return rows(nodes, "").filter((row) => row.kind === "document").length;
}

/** The project's sections: the folders at the top of the tree. */
export function sections(nodes: TreeNode[]) {
	return nodes.filter((node) => node.node === "folder");
}

/** Every document in the tree, wherever it sits. */
export function documentsOf(nodes: TreeNode[]): ProjectDocument[] {
	return rows(nodes, "").flatMap((row) =>
		row.kind === "document" ? [row.document] : [],
	);
}

/** What is on the move, as the list of destinations needs to know it. */
export type Moving = {
	id: string;
	/** A document has no kind and no rules; only a folder's kind is asked for. */
	kind: FolderKind | null;
	folder: boolean;
	/** The folder it sits in now, which is not somewhere to move it to. */
	from: string;
};

/** One folder the writer may aim at, as the menu draws it. */
export type Destination = {
	id: string;
	name: string;
	/** How far in to draw it, the sections being at nought. */
	depth: number;
	/** How many nodes it holds, which is the index that puts one at the end. */
	children: number;
	/** False when the rules turn it down, or when the node is already there. */
	allowed: boolean;
};

/**
 * Every folder in the project, depth first, said to be allowed or not.
 *
 * A node's own subtree is left out rather than drawn refused: it is not a place
 * that exists to be aimed at once the node is inside it. Everything else is
 * listed, so the writer can see the shape of the project while choosing, and
 * `mayHold` says which of them will take this node.
 */
export function destinations(nodes: TreeNode[], moving: Moving): Destination[] {
	function walk(
		level: TreeNode[],
		depth: number,
		section: string,
	): Destination[] {
		return level.flatMap((node) => {
			if (node.node !== "folder" || node.id === moving.id) {
				return [];
			}

			// The section is the top-level folder, so at the top the folder is
			// its own section.
			const under = depth === 0 ? node.name : section;
			const destination: Destination = {
				id: node.id,
				name: node.name,
				depth,
				children: node.children.length,
				allowed:
					node.id !== moving.from &&
					(!moving.folder ||
						mayHold(under === MANUSCRIPT, node.kind, moving.kind)),
			};

			return [destination, ...walk(node.children, depth + 1, under)];
		});
	}

	return walk(nodes, 0, "");
}
