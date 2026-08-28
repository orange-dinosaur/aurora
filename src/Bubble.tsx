import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { $getSelection, $isRangeSelection } from "lexical";
import {
	useCallback,
	useEffect,
	useLayoutEffect,
	useRef,
	useState,
} from "react";
import Controls from "./Controls";

// Where the selected words are, in the surface's own coordinates.
type Anchor = { centre: number; top: number; bottom: number };

// How far the bar keeps off the words it belongs to.
const GAP = 8;

export default function Bubble() {
	const [editor] = useLexicalComposerContext();
	const [at, setAt] = useState<Anchor | null>(null);
	const bubble = useRef<HTMLDivElement>(null);

	const place = useCallback(() => {
		const box = bubble.current;
		const surface = box?.parentElement ?? null;
		const root = editor.getRootElement();
		const native = window.getSelection();
		// Several editors are mounted at once, one per open tab, so the bar
		// only belongs to the one the browser's own selection is inside.
		if (
			box === null ||
			surface === null ||
			root === null ||
			native === null ||
			native.rangeCount === 0 ||
			native.isCollapsed ||
			!root.contains(native.anchorNode)
		) {
			setAt(null);
			return;
		}

		const ranged = editor.getEditorState().read(() => {
			const selection = $getSelection();
			return $isRangeSelection(selection) && !selection.isCollapsed();
		});
		if (!ranged) {
			setAt(null);
			return;
		}

		const words = native.getRangeAt(0).getBoundingClientRect();
		const frame = surface.getBoundingClientRect();
		setAt({
			centre: words.left + words.width / 2 - frame.left,
			top: words.top - frame.top,
			bottom: words.bottom - frame.top,
		});
	}, [editor]);

	useEffect(() => {
		const forget = editor.registerUpdateListener(place);
		// The bar hangs over the text, so it has to follow when the text moves
		// underneath it. Scroll is watched in the capture phase because it is
		// the writing surface that scrolls, not the window.
		window.addEventListener("resize", place);
		document.addEventListener("scroll", place, true);
		return () => {
			forget();
			window.removeEventListener("resize", place);
			document.removeEventListener("scroll", place, true);
		};
	}, [editor, place]);

	// Placed after it has been drawn, because centring it needs its width, and
	// its width changes with the name of the block the caret is in. No
	// dependency list: the buttons hold their own state, so the only reliable
	// moment to measure is after every render.
	useLayoutEffect(() => {
		const box = bubble.current;
		if (box === null || at === null) {
			return;
		}
		const room = box.parentElement?.clientWidth ?? 0;
		const left = Math.min(
			Math.max(at.centre - box.offsetWidth / 2, 0),
			Math.max(room - box.offsetWidth, 0),
		);
		const above = at.top - box.offsetHeight - GAP;
		box.style.left = `${left}px`;
		box.style.top = `${above < 0 ? at.bottom + GAP : above}px`;
	});

	return (
		<div
			ref={bubble}
			className={at === null ? "bubble bubble--hidden" : "bubble"}
		>
			<Controls hidden={at === null} />
		</div>
	);
}
