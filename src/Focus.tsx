import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { $getSelection, $isRangeSelection } from "lexical";
import { useEffect, useRef } from "react";
import { inAField } from "./formatting";

const MARK = "editor__focused";

type Props = {
	on: boolean;
	/** Whether this is the tab being looked at, and so the one Escape answers. */
	active: boolean;
	onLeave: () => void;
};

// Marks the block the caret is in, so the stylesheet can dim the others. The
// mark goes on the DOM element rather than into the document: which paragraph
// is being written in right now is not part of what gets saved.
export default function Focus({ on, active, onLeave }: Props) {
	const [editor] = useLexicalComposerContext();
	const marked = useRef<HTMLElement | null>(null);

	// Focus takes the frame away, so Escape is the way out with the dock hidden.
	// Left to anything that had a use for the key first, such as the slash menu
	// or Find, so closing those does not leave Focus as well.
	useEffect(() => {
		if (!on || !active) {
			return;
		}

		function leave(event: KeyboardEvent) {
			if (
				event.key === "Escape" &&
				!event.defaultPrevented &&
				!inAField(event.target)
			) {
				onLeave();
			}
		}

		window.addEventListener("keydown", leave);
		return () => window.removeEventListener("keydown", leave);
	}, [on, active, onLeave]);

	useEffect(() => {
		function mark(element: HTMLElement | null) {
			if (marked.current !== null && marked.current !== element) {
				marked.current.classList.remove(MARK);
			}
			// Put back after every update rather than only when it moves:
			// Lexical rewrites a block's classes whenever its kind changes,
			// and would take this one with them.
			element?.classList.add(MARK);
			marked.current = element;
		}

		if (!on) {
			mark(null);
			return;
		}

		function follow() {
			const key = editor.getEditorState().read(() => {
				const selection = $getSelection();
				if (!$isRangeSelection(selection)) {
					return null;
				}
				return (
					selection.anchor.getNode().getTopLevelElement()?.getKey() ??
					null
				);
			});
			mark(key === null ? null : editor.getElementByKey(key));
		}

		follow();
		const forget = editor.registerUpdateListener(follow);
		return () => {
			forget();
			mark(null);
		};
	}, [editor, on]);

	return null;
}
