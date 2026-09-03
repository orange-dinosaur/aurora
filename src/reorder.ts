// Drag-to-reorder, shared by the two surfaces that list documents. The
// vocabulary is the same one the menu uses: a drop asks for the folder and the
// index the node should end up at, which is what `move_node` takes. A drag
// only ever stays inside one folder, so the folder it names is the one the
// node is already in.

import { useState } from "react";
import type { DragEvent } from "react";

type Dragging = { id: string; from: number; group: string } | null;

export function useReorder(
	move: (id: string, parentId: string, index: number) => void,
) {
	const [dragging, setDragging] = useState<Dragging>(null);
	const [over, setOver] = useState<number | null>(null);

	function stop() {
		setDragging(null);
		setOver(null);
	}

	/**
	 * What to put on the element standing for one document in the list. The
	 * group is the section it belongs to: a document only ever moves within
	 * its own, so anywhere else refuses the drop rather than quietly meaning
	 * something the writer did not ask for.
	 */
	function item(id: string, at: number, group: string) {
		const landing = dragging !== null && dragging.group === group;

		return {
			draggable: true,
			"data-dragging": dragging?.id === id ? "" : undefined,
			"data-over":
				landing && dragging.from !== at && over === at ? "" : undefined,
			onDragStart(event: DragEvent) {
				event.dataTransfer.effectAllowed = "move";
				// A drag with nothing on the transfer does not start at all in
				// some browsers.
				event.dataTransfer.setData("text/plain", id);
				setDragging({ id, from: at, group });
			},
			onDragOver(event: DragEvent) {
				if (!landing) {
					return;
				}
				// Only a prevented dragover makes an element a drop target, so
				// another section simply never becomes one.
				event.preventDefault();
				event.dataTransfer.dropEffect = "move";
				setOver(at);
			},
			onDragLeave() {
				setOver((current) => (current === at ? null : current));
			},
			onDrop(event: DragEvent) {
				event.preventDefault();
				const from = dragging;
				stop();
				if (landing && from !== null && from.from !== at) {
					move(from.id, group, at);
				}
			},
			onDragEnd: stop,
		};
	}

	return { item };
}
