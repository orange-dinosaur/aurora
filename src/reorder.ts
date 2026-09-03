// Drag-to-move, shared by the two surfaces that list what a folder holds. The
// vocabulary is the menu's: a drop asks for the folder and the index the node
// should end up at, which is what `move_node` takes.
//
// A row answers a drag in one of two ways, depending on where it is aimed. At
// either end it says "before me" or "after me", and the node lands beside it,
// among the nodes it already sits with. At the middle of a folder it says
// "inside me", and the node lands at the end of what that folder holds. A
// document holds nothing, so its middle belongs to its nearer end.

import { useState } from "react";
import type { DragEvent } from "react";
import type { FolderKind } from "./types";
import { MANUSCRIPT, mayHold } from "./kinds";

/** Where a drop would put the node, said against the row under the cursor. */
export type Landing = "before" | "into" | "after";

/** The direction the list runs in: down the sidebar, across the overview. */
type Axis = "x" | "y";

/** One row, as a drag needs to know it. */
export type Spot = {
	id: string;
	/** Its index among the nodes beside it, which is what a drop counts from. */
	at: number;
	/** The folder holding it. */
	group: string;
	/** The section it lives under, which decides whether kinds mean anything. */
	section: string;
	/** Its own kind, which only a folder in the Manuscript has. */
	kind: FolderKind | null;
	folder: boolean;
	/** How many nodes it holds, which is the index that puts one at its end. */
	holds: number;
	/** Whether it is drawn inside the node being dragged, and so is nowhere. */
	within: boolean;
};

/** What goes on the element standing for one row, so a row can ask for it. */
export type Draggable = {
	draggable: boolean;
	"data-dragging": string | undefined;
	"data-over": Landing | undefined;
	onDragStart: (event: DragEvent) => void;
	onDragOver: (event: DragEvent) => void;
	onDragLeave: () => void;
	onDrop: (event: DragEvent) => void;
	onDragEnd: () => void;
};

/**
 * The index a drop would ask for. Landing inside a folder means the end of it;
 * landing beside a row means that row's place, give or take.
 */
export function index(moving: Spot, target: Spot, landing: Landing): number {
	if (landing === "into") {
		return target.holds;
	}

	// The node is lifted out before it comes down, so anywhere past where it
	// was has moved up one by the time it gets there.
	const lifted =
		moving.group === target.group && moving.at < target.at ? 1 : 0;
	return target.at - lifted + (landing === "after" ? 1 : 0);
}

/**
 * Whether this row will take the node, landed that way. Rust holds the same
 * rules and refuses anything else; this is so a drop that would be turned down
 * never draws a cursor.
 *
 * Inside a folder is the only way a node changes hands, and `mayHold` says
 * which folders will have it. Beside a row is a reorder, so it means something
 * only among the nodes the dragged one already sits with: to send it elsewhere,
 * aim at the folder itself.
 */
export function accepts(moving: Spot, target: Spot, landing: Landing): boolean {
	if (moving.id === target.id || target.within) {
		return false;
	}

	if (landing === "into") {
		return (
			target.folder &&
			moving.group !== target.id &&
			(!moving.folder ||
				mayHold(
					target.section === MANUSCRIPT,
					target.kind,
					moving.kind,
				))
		);
	}

	return (
		moving.group === target.group &&
		index(moving, target, landing) !== moving.at
	);
}

/** How far along the row the cursor is, as a fraction of its length. */
function aim(event: DragEvent, along: Axis): number {
	const box = event.currentTarget.getBoundingClientRect();
	return along === "y"
		? (event.clientY - box.top) / box.height
		: (event.clientX - box.left) / box.width;
}

/**
 * What the row under the cursor would do with the drag, or null if it will not
 * take it. The ends are a quarter of the row each where the middle stands for
 * something, and half each where it does not.
 */
function landing(
	event: DragEvent,
	moving: Spot,
	target: Spot,
	along: Axis,
): Landing | null {
	const fraction = aim(event, along);
	const edge = target.folder ? 0.25 : 0.5;
	const asked: Landing =
		fraction < edge
			? "before"
			: fraction > 1 - edge
				? "after"
				: target.folder
					? "into"
					: "after";

	return accepts(moving, target, asked) ? asked : null;
}

export function useReorder(
	move: (id: string, parentId: string, index: number) => void,
	along: Axis = "y",
) {
	const [dragging, setDragging] = useState<Spot | null>(null);
	const [over, setOver] = useState<{ id: string; landing: Landing } | null>(
		null,
	);

	function stop() {
		setDragging(null);
		setOver(null);
	}

	function clear(id: string) {
		setOver((current) => (current?.id === id ? null : current));
	}

	/** What to put on the element standing for one row in the list. */
	function item(spot: Spot): Draggable {
		return {
			draggable: true,
			"data-dragging": dragging?.id === spot.id ? "" : undefined,
			"data-over": over?.id === spot.id ? over.landing : undefined,
			onDragStart(event: DragEvent) {
				event.dataTransfer.effectAllowed = "move";
				// A drag with nothing on the transfer does not start at all in
				// some browsers.
				event.dataTransfer.setData("text/plain", spot.id);
				setDragging(spot);
			},
			onDragOver(event: DragEvent) {
				const asked =
					dragging === null
						? null
						: landing(event, dragging, spot, along);
				if (asked === null) {
					clear(spot.id);
					return;
				}

				// Only a prevented dragover makes an element a drop target, so
				// a drop the rules turn down is refused by saying nothing.
				event.preventDefault();
				event.dataTransfer.dropEffect = "move";
				// Held to the same object while the answer holds, since
				// dragover fires all the while the cursor is over the row.
				setOver((current) =>
					current?.id === spot.id && current.landing === asked
						? current
						: { id: spot.id, landing: asked },
				);
			},
			onDragLeave() {
				clear(spot.id);
			},
			onDrop(event: DragEvent) {
				event.preventDefault();
				const moving = dragging;
				stop();
				if (moving === null) {
					return;
				}

				const asked = landing(event, moving, spot, along);
				if (asked !== null) {
					move(
						moving.id,
						asked === "into" ? spot.id : spot.group,
						index(moving, spot, asked),
					);
				}
			},
			onDragEnd: stop,
		};
	}

	return { item, dragging: dragging?.id ?? null };
}
