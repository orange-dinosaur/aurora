// The project's tree as a list the sidebar can draw. A nested list would mean
// a component that calls itself; one flat list of rows, each carrying how deep
// it sits, keeps the drawing in one place and the walking here, where it can
// be tested without a screen.

import type { ProjectDocument, TreeNode } from "./types";

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
	| (Place & { kind: "folder"; id: string; name: string })
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
