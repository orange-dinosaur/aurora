import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { $getSelection, $isRangeSelection, type LexicalEditor } from "lexical";
import { useEffect, useRef, useState } from "react";
import {
	$formattingOf,
	BLOCKS,
	MARKS,
	sameFormatting,
	type Action,
	type Formatting,
} from "./formatting";

// What the caret is sitting in, kept in step with the editor. The reading is
// only replaced when the answer actually changes, so a bar does not redraw
// itself on every keystroke.
function useFormatting(editor: LexicalEditor): Formatting | null {
	const [formatting, setFormatting] = useState<Formatting | null>(null);

	useEffect(() => {
		function refresh() {
			const now = editor.getEditorState().read(() => {
				const selection = $getSelection();
				return $isRangeSelection(selection)
					? $formattingOf(selection)
					: null;
			});
			setFormatting((was) => (sameFormatting(was, now) ? was : now));
		}

		refresh();
		return editor.registerUpdateListener(refresh);
	}, [editor]);

	return formatting;
}

type Props = {
	// The floating bar goes on existing while it is out of sight, so it has to
	// be told to put its menu away.
	hidden?: boolean;
};

// The row of buttons itself, shared by the bar under the title and the one that
// floats over a selection, so the two can never offer different things.
export default function Controls({ hidden = false }: Props) {
	const [editor] = useLexicalComposerContext();
	const formatting = useFormatting(editor);
	const [open, setOpen] = useState(false);
	const block = useRef<HTMLDivElement>(null);

	useEffect(() => {
		if (hidden) {
			setOpen(false);
		}
	}, [hidden]);

	// Dismissal is watched on the document rather than through the menu losing
	// focus: this webview does not focus a button when it is clicked, so a blur
	// handler would close on mousedown and the click that followed would land
	// on the text underneath.
	useEffect(() => {
		if (!open) {
			return;
		}

		function away(event: MouseEvent) {
			const at = event.target;
			if (!(at instanceof Node) || !block.current?.contains(at)) {
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

	function act(action: Action) {
		setOpen(false);
		// The bar is not part of the writing, so a press hands the caret
		// straight back to the text. If there has never been one — a document
		// opened and formatted without being clicked in — it goes to the top
		// rather than nowhere, so the button always does something.
		editor.focus(() => action.run(editor), {
			defaultSelection: "rootStart",
		});
	}

	const name =
		formatting === null ? "Text" : (formatting.block?.label ?? "Mixed");

	return (
		<div
			className="controls"
			role="toolbar"
			aria-label="Formatting"
			// Pressing a button here must not take the selection away from the
			// words the button is about to format.
			onMouseDown={(event) => event.preventDefault()}
		>
			<div ref={block} className="controls__block">
				<button
					type="button"
					className="controls__turn"
					aria-expanded={open}
					onClick={() => setOpen((was) => !was)}
				>
					{name}
					<span className="controls__caret" aria-hidden="true">
						▾
					</span>
				</button>

				{open && (
					<div className="controls__list" role="menu">
						{BLOCKS.map((action) => (
							<button
								key={action.id}
								type="button"
								role="menuitemradio"
								aria-checked={formatting?.block === action}
								className="controls__option"
								onClick={() => act(action)}
							>
								{action.label}
							</button>
						))}
					</div>
				)}
			</div>

			<span className="controls__divider" aria-hidden="true" />

			{MARKS.map((action) => (
				<button
					key={action.id}
					type="button"
					className={`controls__mark controls__mark--${action.id}`}
					aria-label={action.label}
					aria-pressed={formatting?.marks.has(action.id) ?? false}
					onClick={() => act(action)}
				>
					{action.glyph}
				</button>
			))}
		</div>
	);
}
