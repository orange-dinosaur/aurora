import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import {
	$getRoot,
	$getSelection,
	$isElementNode,
	$isRangeSelection,
	HISTORY_MERGE_TAG,
	SKIP_DOM_SELECTION_TAG,
} from "lexical";
import { useEffect, useRef, type RefObject } from "react";
import type { Caret } from "./bindings/Caret";
import { locate, type Place } from "./workspace";

type Props = {
	/** Where the document was left, put back once when the editor mounts. */
	place: Place | null;
	active: boolean;
	scroller: RefObject<HTMLDivElement | null>;
	onCaret: (caret: Caret | null) => void;
};

// Puts the caret and the page back where the writer left them, then reports
// the caret every time it moves.
export default function Whereabouts({
	place,
	active,
	scroller,
	onCaret,
}: Props) {
	const [editor] = useLexicalComposerContext();
	const back = useRef(place);
	const front = useRef(active);

	useEffect(() => {
		const left = back.current;
		if (left !== null) {
			const caret = left.caret;
			if (caret !== null) {
				// Not an edit and not something to undo. The DOM selection is
				// left alone so a tab behind the others does not take focus.
				editor.update(() => $putCaret(caret), {
					tag: [SKIP_DOM_SELECTION_TAG, HISTORY_MERGE_TAG],
				});
				if (front.current) {
					editor.focus();
				}
			}
			// After focus, which scrolls the caret into view, and after the
			// text is laid out, so the page lands where it was.
			requestAnimationFrame(() => {
				if (scroller.current !== null) {
					scroller.current.scrollTop = left.scroll;
				}
			});
		}

		return editor.registerUpdateListener(({ editorState }) =>
			onCaret(editorState.read($caret)),
		);
	}, [editor, scroller, onCaret]);

	return null;
}

function $caret(): Caret | null {
	const selection = $getSelection();
	if (!$isRangeSelection(selection)) {
		return null;
	}
	const anchor = selection.anchor;
	const node = anchor.getNode();
	const top = node.getTopLevelElement();
	if (top === null) {
		return null;
	}

	// ponytail: a caret sitting between elements rather than in a run of
	// text, which an empty paragraph is, goes back to the paragraph's start.
	let offset = 0;
	if (anchor.type === "text" && $isElementNode(top)) {
		for (const text of top.getAllTextNodes()) {
			if (text.is(node)) {
				offset += anchor.offset;
				break;
			}
			offset += text.getTextContentSize();
		}
	}
	return { paragraph: top.getIndexWithinParent(), offset };
}

function $putCaret(caret: Caret) {
	const root = $getRoot();
	// The file may have lost paragraphs since, so the last one stands in.
	const top = root.getChildAtIndex(caret.paragraph) ?? root.getLastChild();
	if (!$isElementNode(top)) {
		return;
	}
	const texts = top.getAllTextNodes();
	if (texts.length === 0) {
		top.selectStart();
		return;
	}
	const at = locate(
		texts.map((text) => text.getTextContentSize()),
		caret.offset,
	);
	texts[at.run].select(at.offset, at.offset);
}
