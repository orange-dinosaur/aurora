import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { useEffect } from "react";

/** How far down the writing surface the caret's line is held. */
const LINE = 0.4;

/** Below this the correction is not worth making, and making it every time
 * would shiver the page while a word is typed. */
const SETTLED = 1;

type Props = {
	on: boolean;
};

// Holds the line being written on at a fixed height and moves the page under
// it, instead of letting the caret walk down to the bottom edge.
export default function Typewriter({ on }: Props) {
	const [editor] = useLexicalComposerContext();

	useEffect(() => {
		if (!on) {
			return;
		}

		function hold() {
			const root = editor.getRootElement();
			const native = window.getSelection();
			// Every open document has a mounted editor, so only the one the
			// browser's own selection is inside may scroll itself.
			if (
				root === null ||
				native === null ||
				native.rangeCount === 0 ||
				!root.contains(native.anchorNode)
			) {
				return;
			}

			const caret = native.getRangeAt(0).getBoundingClientRect();
			// A collapsed range in an empty block measures zero on every side,
			// so there is nothing to aim at.
			if (caret.top === 0 && caret.bottom === 0) {
				return;
			}

			const frame = root.getBoundingClientRect();
			const drift = caret.top - (frame.top + frame.height * LINE);
			if (Math.abs(drift) >= SETTLED) {
				root.scrollTop += drift;
			}
		}

		hold();
		return editor.registerUpdateListener(hold);
	}, [editor, on]);

	return null;
}
