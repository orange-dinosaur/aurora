// Drag-to-reorder, shared by the two surfaces that list documents. The
// vocabulary is the same one the menu uses: a drop asks for the index the
// document should end up at, which is what `reorder_document` takes.

import { useState } from "react";
import type { DragEvent } from "react";

type Dragging = { id: string; from: number } | null;

export function useReorder(move: (id: string, index: number) => void) {
	const [dragging, setDragging] = useState<Dragging>(null);
	const [over, setOver] = useState<number | null>(null);

	function stop() {
		setDragging(null);
		setOver(null);
	}

	/** What to put on the element standing for one document in the list. */
	function item(id: string, at: number) {
		return {
			draggable: true,
			"data-dragging": dragging?.id === id ? "" : undefined,
			"data-over":
				dragging !== null && dragging.from !== at && over === at
					? ""
					: undefined,
			onDragStart(event: DragEvent) {
				event.dataTransfer.effectAllowed = "move";
				// A drag with nothing on the transfer does not start at all in
				// some browsers.
				event.dataTransfer.setData("text/plain", id);
				setDragging({ id, from: at });
			},
			onDragOver(event: DragEvent) {
				if (dragging === null) {
					return;
				}
				// Only a prevented dragover makes an element a drop target.
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
				if (from !== null && from.from !== at) {
					move(from.id, at);
				}
			},
			onDragEnd: stop,
		};
	}

	return { item };
}
