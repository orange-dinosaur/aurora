import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import {
	$getRoot,
	CLICK_COMMAND,
	COMMAND_PRIORITY_LOW,
	IS_APPLE,
} from "lexical";
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { failure } from "./errors";
import { missingFrom, type Missing } from "./screenplay";
import { NO_SCRIPT_NAMES, type ScriptNames } from "./subjects";

const CLICK = IS_APPLE ? "⌘-click" : "Ctrl+click";

type Props = {
	cast: { names: ScriptNames; ask: () => void };
	/** A cue or heading Ctrl+clicked, with the section its page would be in. */
	onFollow: (section: Missing["section"], line: string) => void;
	/** Makes the page a missing name asks for. */
	onAdd: (missing: Missing) => Promise<void>;
};

type Offer = Missing & { x: number; y: number; refused: string | null };

/**
 * The script's people and places. A cue or a heading naming no page is
 * underlined, and Ctrl+click on it offers to make the page; Ctrl+click on a
 * cue that does name one opens it.
 */
export default function ScriptSubjects({ cast, onFollow, onAdd }: Props) {
	const [editor] = useLexicalComposerContext();
	const [offer, setOffer] = useState<Offer | null>(null);
	const box = useRef<HTMLDivElement>(null);
	const { names, ask } = cast;

	useEffect(() => {
		ask();
	}, [ask]);

	// Marked on the lines' own elements after every update, since whether a
	// name is missing depends on the project and not on anything in the text.
	useEffect(() => {
		if (names === NO_SCRIPT_NAMES) {
			return;
		}
		function mark() {
			editor.getEditorState().read(() => {
				for (const node of $getRoot().getChildren()) {
					const line = editor.getElementByKey(node.getKey());
					if (line === null) {
						continue;
					}
					const missing = missingFrom(
						line.dataset.kind,
						node.getTextContent(),
						names,
					);
					if (missing === null) {
						delete line.dataset.missing;
						line.removeAttribute("title");
					} else {
						line.dataset.missing = missing.section;
						line.title = `${CLICK} to add ${missing.name} to ${missing.section}`;
					}
				}
			});
		}
		mark();
		return editor.registerUpdateListener(mark);
	}, [editor, names]);

	// A plain click puts the caret in the line; holding the command key follows
	// it, as it follows a link.
	useEffect(
		() =>
			editor.registerCommand(
				CLICK_COMMAND,
				(event) => {
					if (!event.ctrlKey && !event.metaKey) {
						return false;
					}
					const at = event.target;
					const line =
						at instanceof Element
							? at.closest<HTMLElement>(
									'[data-kind="character"], [data-kind="heading"]',
								)
							: null;
					if (line === null) {
						return false;
					}
					const missing = missingFrom(
						line.dataset.kind,
						line.textContent ?? "",
						names,
					);
					event.preventDefault();
					if (missing === null) {
						onFollow(
							line.dataset.kind === "character"
								? "Characters"
								: "Locations",
							line.textContent ?? "",
						);
					} else {
						setOffer({
							...missing,
							x: event.clientX,
							y: event.clientY,
							refused: null,
						});
					}
					return true;
				},
				COMMAND_PRIORITY_LOW,
			),
		[editor, names, onFollow],
	);

	useEffect(() => {
		if (offer === null) {
			return;
		}
		function away(event: MouseEvent) {
			const at = event.target;
			if (!(at instanceof Node) || !box.current?.contains(at)) {
				setOffer(null);
			}
		}
		function key(event: KeyboardEvent) {
			if (event.key === "Escape") {
				event.preventDefault();
				setOffer(null);
			}
		}
		document.addEventListener("mousedown", away);
		window.addEventListener("keydown", key, true);
		return () => {
			document.removeEventListener("mousedown", away);
			window.removeEventListener("keydown", key, true);
		};
	}, [offer]);

	if (offer === null) {
		return null;
	}

	async function add(missing: Offer) {
		try {
			await onAdd({ section: missing.section, name: missing.name });
			setOffer(null);
		} catch (error) {
			setOffer({ ...missing, refused: failure(error).message });
		}
	}

	return createPortal(
		<div
			ref={box}
			className="script__add"
			role="dialog"
			// Kept inside the window when the click was near its right edge.
			style={{
				left: Math.min(offer.x, window.innerWidth - 272),
				top: offer.y + 12,
			}}
		>
			<button
				type="button"
				className="slash__option"
				autoFocus
				onClick={() => void add(offer)}
			>
				Add {offer.name} to {offer.section}
			</button>
			{offer.refused !== null && (
				<p className="script__refused">{offer.refused}</p>
			)}
		</div>,
		document.body,
	);
}
