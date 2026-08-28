import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { $getSelection, $isRangeSelection } from "lexical";
import {
	useCallback,
	useEffect,
	useLayoutEffect,
	useRef,
	useState,
} from "react";
import { $formattingOf, BLOCKS, MARKS, type Action } from "./formatting";

// Where the bar should sit, in the surface's own coordinates, alongside what it
// should be showing. One piece of state, so the bar never draws a reading of
// one selection at the position of another.
type Shown = {
	marks: ReadonlySet<string>;
	block: Action | null;
	centre: number;
	top: number;
	bottom: number;
};

// How far the bar keeps off the words it belongs to.
const GAP = 8;

export default function Bubble() {
	const [editor] = useLexicalComposerContext();
	const [shown, setShown] = useState<Shown | null>(null);
	const [open, setOpen] = useState(false);
	const bubble = useRef<HTMLDivElement>(null);

	const refresh = useCallback(() => {
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
			setShown(null);
			setOpen(false);
			return;
		}

		const reading = editor.getEditorState().read(() => {
			const selection = $getSelection();
			return $isRangeSelection(selection) && !selection.isCollapsed()
				? $formattingOf(selection)
				: null;
		});
		if (reading === null) {
			setShown(null);
			setOpen(false);
			return;
		}

		const words = native.getRangeAt(0).getBoundingClientRect();
		const frame = surface.getBoundingClientRect();
		setShown({
			marks: reading.marks,
			block: reading.block,
			centre: words.left + words.width / 2 - frame.left,
			top: words.top - frame.top,
			bottom: words.bottom - frame.top,
		});
	}, [editor]);

	useEffect(() => {
		const forget = editor.registerUpdateListener(refresh);
		// The bar hangs over the text, so it has to follow when the text moves
		// underneath it. Scroll is watched in the capture phase because it is
		// the writing surface that scrolls, not the window.
		window.addEventListener("resize", refresh);
		document.addEventListener("scroll", refresh, true);
		return () => {
			forget();
			window.removeEventListener("resize", refresh);
			document.removeEventListener("scroll", refresh, true);
		};
	}, [editor, refresh]);

	// Placed after it has been drawn, because centring it needs its width, and
	// its width changes with the name of the block the caret is in.
	useLayoutEffect(() => {
		const box = bubble.current;
		if (box === null || shown === null) {
			return;
		}
		const room = box.parentElement?.clientWidth ?? 0;
		const left = Math.min(
			Math.max(shown.centre - box.offsetWidth / 2, 0),
			Math.max(room - box.offsetWidth, 0),
		);
		const above = shown.top - box.offsetHeight - GAP;
		box.style.left = `${left}px`;
		box.style.top = `${above < 0 ? shown.bottom + GAP : above}px`;
	}, [shown]);

	// Dismissal is watched on the document rather than through the dropdown
	// losing focus: this webview does not focus a button when it is clicked, so
	// a blur handler would close on mousedown and the click that followed would
	// land on the text underneath.
	useEffect(() => {
		if (!open) {
			return;
		}

		function away(event: MouseEvent) {
			const at = event.target;
			if (!(at instanceof Node) || !bubble.current?.contains(at)) {
				setOpen(false);
			}
		}

		function escape(event: KeyboardEvent) {
			if (event.key === "Escape") {
				setOpen(false);
			}
		}

		document.addEventListener("mousedown", away);
		document.addEventListener("keydown", escape);
		return () => {
			document.removeEventListener("mousedown", away);
			document.removeEventListener("keydown", escape);
		};
	}, [open]);

	return (
		<div
			ref={bubble}
			className={shown === null ? "bubble bubble--hidden" : "bubble"}
			// Pressing a button here must not take the selection away from the
			// words the button is about to format.
			onMouseDown={(event) => event.preventDefault()}
		>
			<div className="bubble__block">
				<button
					type="button"
					className="bubble__turn"
					aria-expanded={open}
					onClick={() => setOpen((was) => !was)}
				>
					{shown?.block?.label ?? "Mixed"}
					<span className="bubble__caret" aria-hidden="true">
						▾
					</span>
				</button>

				{open && (
					<div className="bubble__list" role="menu">
						{BLOCKS.map((action) => (
							<button
								key={action.id}
								type="button"
								role="menuitemradio"
								aria-checked={shown?.block?.id === action.id}
								className="bubble__option"
								onClick={() => {
									setOpen(false);
									action.run(editor);
								}}
							>
								{action.label}
							</button>
						))}
					</div>
				)}
			</div>

			<span className="bubble__divider" aria-hidden="true" />

			{MARKS.map((action) => (
				<button
					key={action.id}
					type="button"
					className={`bubble__mark bubble__mark--${action.id}`}
					aria-label={action.label}
					aria-pressed={shown?.marks.has(action.id) ?? false}
					onClick={() => action.run(editor)}
				>
					{action.glyph}
				</button>
			))}
		</div>
	);
}
