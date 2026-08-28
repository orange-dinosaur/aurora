import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import {
	$createRangeSelection,
	$getNodeByKey,
	$getRoot,
	$setSelection,
	TextNode,
	type LexicalEditor,
} from "lexical";
import { useEffect, useRef, useState } from "react";
import { matches, type Match, type Run } from "./find";

type Props = {
	/** Counts the times find has been asked for, so a second ask can answer. */
	asked: number;
	onClose: () => void;
};

// Two paint layers rather than one: the match being stepped through has to
// stand out from the others without anything in the document changing.
const OTHERS = "aurora-find";
const CURRENT = "aurora-find-current";

/** The runs of text the document is made of, in reading order. */
function $runs(): Run[] {
	return $getRoot()
		.getAllTextNodes()
		.map((node) => ({
			key: node.getKey(),
			text: node.getTextContent(),
			block: node.getTopLevelElementOrThrow().getKey(),
		}));
}

// Lexical renders every run of text as a span with a single text node inside
// it, which is what a Range has to be built from.
function textOf(element: HTMLElement | null): Text | null {
	const first = element?.firstChild;
	return first instanceof Text ? first : null;
}

function ranged(editor: LexicalEditor, match: Match): Range | null {
	const from = textOf(editor.getElementByKey(match.fromKey));
	const to = textOf(editor.getElementByKey(match.toKey));
	if (from === null || to === null) {
		return null;
	}

	const range = document.createRange();
	range.setStart(from, Math.min(match.fromOffset, from.length));
	range.setEnd(to, Math.min(match.toOffset, to.length));
	return range;
}

function clear() {
	if ("highlights" in CSS) {
		CSS.highlights.delete(OTHERS);
		CSS.highlights.delete(CURRENT);
	}
}

/** Tints every match, the current one differently. False if the webview cannot. */
function paint(editor: LexicalEditor, found: Match[], at: number): boolean {
	if (!("highlights" in CSS)) {
		return false;
	}

	const others: Range[] = [];
	const current: Range[] = [];
	found.forEach((match, index) => {
		const range = ranged(editor, match);
		if (range !== null) {
			(index === at ? current : others).push(range);
		}
	});

	CSS.highlights.set(OTHERS, new Highlight(...others));
	CSS.highlights.set(CURRENT, new Highlight(...current));
	return true;
}

/** Puts the caret on a match, which is where the writer carries on from. */
function land(editor: LexicalEditor, match: Match) {
	editor.update(() => {
		const from = $getNodeByKey(match.fromKey);
		const to = $getNodeByKey(match.toKey);
		if (!(from instanceof TextNode) || !(to instanceof TextNode)) {
			return;
		}

		const selection = $createRangeSelection();
		selection.anchor.set(match.fromKey, match.fromOffset, "text");
		selection.focus.set(match.toKey, match.toOffset, "text");
		$setSelection(selection);
	});
	editor.focus();
}

/**
 * Find over the open document. The caret stays in the field while the writer
 * steps through the matches, and lands on the one they were on when the panel
 * closes.
 */
export default function Find({ asked, onClose }: Props) {
	const [editor] = useLexicalComposerContext();
	const [query, setQuery] = useState("");
	const [found, setFound] = useState<Match[]>([]);
	const [at, setAt] = useState(0);
	const input = useRef<HTMLInputElement>(null);

	// Whether the panel is opening or was already open, the field takes the
	// caret and offers up the last query to be typed over.
	useEffect(() => {
		input.current?.focus();
		input.current?.select();
	}, [asked]);

	// Matches are read from the document rather than kept: an edit, an undo or
	// a document arriving from disk all have to be answered the same way.
	useEffect(() => {
		function look() {
			const runs = editor.getEditorState().read($runs);
			setFound(matches(runs, query));
		}

		look();
		return editor.registerUpdateListener(look);
	}, [editor, query]);

	const here = found.length === 0 ? 0 : Math.min(at, found.length - 1);
	const match = found[here];

	useEffect(() => {
		if (match === undefined) {
			clear();
			return;
		}

		paint(editor, found, here);
		editor.getElementByKey(match.fromKey)?.scrollIntoView({
			block: "center",
		});
	}, [editor, found, here, match]);

	// The tint belongs to the panel, not to the document, so it goes when the
	// panel does.
	useEffect(() => clear, []);

	function step(by: number) {
		if (found.length === 0) {
			return;
		}

		const next = (here + by + found.length) % found.length;
		setAt(next);

		// A webview with no paint layer has nothing to show, so there the
		// selection stands in for it. Only on a step, never while the writer
		// is still typing — landing takes the caret out of the field.
		const landing = found[next];
		if (!("highlights" in CSS) && landing !== undefined) {
			land(editor, landing);
		}
	}

	function close() {
		if (match !== undefined) {
			land(editor, match);
		}
		onClose();
	}

	return (
		<div className="editor__find">
			<input
				ref={input}
				className="editor__find-input"
				value={query}
				aria-label="Find in this document"
				placeholder="Find"
				onChange={(event) => {
					setQuery(event.target.value);
					setAt(0);
				}}
				onKeyDown={(event) => {
					if (event.key === "Escape") {
						event.preventDefault();
						close();
					}
					if (event.key === "Enter") {
						event.preventDefault();
						step(event.shiftKey ? -1 : 1);
					}
				}}
			/>
			{/* Always in the DOM, so counting up as the writer types cannot
			    move the buttons under their hand. */}
			<span className="editor__find-count" role="status">
				{query === ""
					? ""
					: found.length === 0
						? "None"
						: `${here + 1} of ${found.length}`}
			</span>
			<button
				type="button"
				className="editor__find-step"
				aria-label="Previous match"
				title="Previous match (Shift+Enter)"
				disabled={found.length === 0}
				onClick={() => step(-1)}
			>
				{"‹"}
			</button>
			<button
				type="button"
				className="editor__find-step"
				aria-label="Next match"
				title="Next match (Enter)"
				disabled={found.length === 0}
				onClick={() => step(1)}
			>
				{"›"}
			</button>
			<button
				type="button"
				className="editor__find-step"
				aria-label="Close find"
				title="Close find (Escape)"
				onClick={close}
			>
				{"✕"}
			</button>
		</div>
	);
}
