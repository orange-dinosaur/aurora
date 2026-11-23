import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { $getSelection, $isRangeSelection, type LexicalEditor } from "lexical";
import { useEffect, useRef, useState } from "react";
import {
	$formattingOf,
	BLOCKS,
	INSERTS,
	MARKS,
	SCRIPT_MARKS,
	linkTarget,
	sameFormatting,
	setLink,
	shortcutLabel,
	type Action,
	type Formatting,
} from "./formatting";
import Icon from "./Icon";
import { walkMenu } from "./Menu";

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
	// Asks a bar that follows the selection to stay where it is. Typing an
	// address means focus leaves the text, which would otherwise move the bar
	// out from under the field being typed in.
	onHold?: (held: boolean) => void;
	// A screenplay scene, which has its three marks and nothing else.
	script?: boolean;
};

// The row of buttons itself, shared by the dock and the bar that floats over a
// selection, so the two can never offer different things.
export default function Controls({
	hidden = false,
	onHold,
	script = false,
}: Props) {
	const [editor] = useLexicalComposerContext();
	const formatting = useFormatting(editor);
	const [open, setOpen] = useState(false);
	// The address being typed, or null when the field is not open. An empty
	// string is a field the writer has cleared, which is how a link is removed.
	const [address, setAddress] = useState<string | null>(null);
	const block = useRef<HTMLDivElement>(null);
	const turn = useRef<HTMLButtonElement>(null);
	const list = useRef<HTMLDivElement>(null);
	// Whether the keyboard opened the menu, which is what puts focus in it.
	const byKey = useRef(false);

	// A press on Escape closes the menu, and focus goes back to the button
	// when it was inside the list.
	function shut() {
		if (list.current?.contains(document.activeElement)) {
			turn.current?.focus();
		}
		setOpen(false);
	}

	useEffect(() => {
		if (open && byKey.current) {
			list.current
				?.querySelector<HTMLButtonElement>("button:enabled")
				?.focus();
		}
	}, [open]);

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
				shut();
			}
		}

		document.addEventListener("mousedown", away);
		document.addEventListener("keydown", escape);
		return () => {
			document.removeEventListener("mousedown", away);
			document.removeEventListener("keydown", escape);
		};
	}, [open]);

	function askForAddress() {
		setOpen(false);
		onHold?.(true);
		setAddress(formatting?.link ?? "");
	}

	function closeAddress() {
		setAddress(null);
		onHold?.(false);
	}

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

	function option(action: Action, checked?: boolean) {
		return (
			<button
				key={action.id}
				type="button"
				role={checked === undefined ? "menuitem" : "menuitemradio"}
				aria-checked={checked}
				className="controls__option"
				onClick={() => act(action)}
			>
				{action.label}
				{/* The menu is where the shortcuts are learned, so they are
				    shown rather than hidden in a tooltip. */}
				<span className="controls__keys">
					{shortcutLabel(action.keys)}
				</span>
			</button>
		);
	}

	const marks = (script ? SCRIPT_MARKS : MARKS).map((action) => (
		<button
			key={action.id}
			type="button"
			className={`controls__mark controls__mark--${action.id}`}
			aria-label={action.label}
			title={`${action.label} (${shortcutLabel(action.keys)})`}
			aria-keyshortcuts={shortcutLabel(action.keys)}
			aria-pressed={formatting?.marks.has(action.id) ?? false}
			onClick={() => act(action)}
		>
			{action.glyph}
		</button>
	));

	if (script) {
		return (
			<div
				className="controls"
				role="toolbar"
				aria-label="Formatting"
				// Keeps the selection on the words being formatted.
				onMouseDown={(event) => event.preventDefault()}
			>
				{marks}
			</div>
		);
	}

	return (
		<div
			className="controls"
			role="toolbar"
			aria-label="Formatting"
			onMouseDown={(event) => {
				// Pressing a button here must not take the selection away from
				// the words the button is about to format. The address field
				// is the exception: it is a real input and has to be clickable.
				if (!(event.target instanceof HTMLInputElement)) {
					event.preventDefault();
				}
			}}
		>
			<div ref={block} className="controls__block">
				<button
					ref={turn}
					type="button"
					className="controls__turn"
					aria-expanded={open}
					onClick={(event) => {
						byKey.current = event.detail === 0;
						setOpen((was) => !was);
					}}
				>
					{name}
					<span className="controls__caret">
						<Icon name="chevron-down" />
					</span>
				</button>

				{open && (
					<div
						ref={list}
						className="controls__list"
						role="menu"
						onKeyDown={(event) =>
							walkMenu(event, () => setOpen(false))
						}
					>
						{BLOCKS.map((action) =>
							option(action, formatting?.block === action),
						)}
						<span className="menu__rule" />
						{INSERTS.map((action) => option(action))}
					</div>
				)}
			</div>

			<span className="controls__divider" aria-hidden="true" />

			{marks}

			<div className="controls__link">
				<button
					type="button"
					className="controls__mark"
					aria-label="Link"
					title="Link"
					aria-expanded={address !== null}
					aria-pressed={
						formatting?.link !== undefined &&
						formatting?.link !== null
					}
					onClick={() =>
						address === null ? askForAddress() : closeAddress()
					}
				>
					<Icon name="link" />
				</button>

				{address !== null && (
					<form
						className="controls__address"
						onSubmit={(event) => {
							event.preventDefault();
							const url = linkTarget(address);
							closeAddress();
							// An emptied field means the link comes off. The
							// caret goes back to the words first: it is what
							// the change is applied to.
							editor.focus(
								() => setLink(editor, url === "" ? null : url),
								{ defaultSelection: "rootStart" },
							);
						}}
						onBlur={(event) => {
							// Clicking away abandons it, as Escape does. An
							// input is focused when it is clicked even in this
							// webview, so blur is safe here.
							const moved = event.relatedTarget;
							if (
								!(moved instanceof Node) ||
								!event.currentTarget.contains(moved)
							) {
								closeAddress();
							}
						}}
					>
						<input
							className="controls__url"
							autoFocus
							value={address}
							aria-label="Link address"
							placeholder="https://…"
							onChange={(event) => setAddress(event.target.value)}
							onKeyDown={(event) => {
								if (event.key === "Escape") {
									event.preventDefault();
									closeAddress();
									editor.focus();
								}
							}}
						/>
					</form>
				)}
			</div>
		</div>
	);
}
