import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import {
	LexicalTypeaheadMenuPlugin,
	MenuOption,
	type MenuTextMatch,
} from "@lexical/react/LexicalTypeaheadMenuPlugin";
import {
	COMMAND_PRIORITY_NORMAL,
	KEY_ESCAPE_COMMAND,
	type LexicalEditor,
} from "lexical";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { shortcutLabel } from "./formatting";
import { scriptEntries } from "./screenplay";
import { $slashAt, matching, NOVEL, type Entry } from "./slash";
import type { ScriptNames } from "./subjects";

// The menu is handed options rather than entries, so each one carries the
// entry it stands for.
class Choice extends MenuOption {
	readonly action: Entry;

	constructor(action: Entry) {
		super(action.id);
		this.action = action;
	}
}

type Props = {
	/** In a script, the people and places to offer, read once a slash is typed. */
	cast?: { names: ScriptNames; ask: () => void };
};

// A slash at the start of an empty block offers the same blocks the bar's
// dropdown does, without the writer having to reach for it.
export default function SlashMenu({ cast }: Props) {
	const [editor] = useLexicalComposerContext();
	const names = cast?.names;
	const offered = useMemo(
		() =>
			names === undefined
				? NOVEL
				: scriptEntries(names.characters, names.locations),
		[names],
	);

	// Nothing to match means no menu at all, rather than an empty one: an open
	// menu takes the arrow keys, and the caret would stop moving.
	const trigger = useCallback(
		(_text: string, editor: LexicalEditor): MenuTextMatch | null => {
			const query = editor.getEditorState().read($slashAt);
			if (query === null || matching(query, offered).length === 0) {
				return null;
			}
			return {
				leadOffset: 0,
				matchingString: query,
				replaceableString: `/${query}`,
			};
		},
		[offered],
	);
	// What has been typed after the slash. Read here as well as by the plugin
	// below, because it is also what says when a menu put away by hand may
	// come back.
	const [typed, setTyped] = useState<string | null>(null);
	const [dismissed, setDismissed] = useState(false);
	const menu = useRef<HTMLDivElement>(null);

	useEffect(
		() =>
			editor.registerUpdateListener(() => {
				const now = editor.getEditorState().read($slashAt);
				setTyped((was) => (was === now ? was : now));
			}),
		[editor],
	);

	useEffect(() => {
		setDismissed(false);
	}, [typed]);

	const asking = typed !== null;
	const ask = cast?.ask;
	useEffect(() => {
		if (asking) {
			ask?.();
		}
	}, [asking, ask]);

	const options = useMemo(
		() =>
			matching(typed ?? "", offered).map((action) => new Choice(action)),
		[typed, offered],
	);

	const showing = typed !== null && !dismissed && options.length > 0;
	// A bare slash leaves the names out, so it says they are there to be found.
	const hinted =
		typed === "" &&
		names !== undefined &&
		names.characters.length + names.locations.length > 0;

	// Watched on the document rather than through the menu losing focus: this
	// webview does not focus a button when it is clicked, so a blur handler
	// would close the menu on mousedown and the click that followed would land
	// on the writing underneath.
	useEffect(() => {
		if (!showing) {
			return;
		}

		function away(event: MouseEvent) {
			const at = event.target;
			if (!(at instanceof Node) || !menu.current?.contains(at)) {
				setDismissed(true);
			}
		}

		document.addEventListener("mousedown", away);
		return () => document.removeEventListener("mousedown", away);
	}, [showing]);

	// Taken above the menu's own handler so that Escape means what clicking
	// away means: gone, and not back on the next keystroke.
	useEffect(() => {
		if (!showing) {
			return;
		}
		return editor.registerCommand(
			KEY_ESCAPE_COMMAND,
			(event) => {
				event.preventDefault();
				setDismissed(true);
				return true;
			},
			COMMAND_PRIORITY_NORMAL,
		);
	}, [editor, showing]);

	// Put away by hand, the menu is taken down rather than hidden: while it is
	// mounted it answers for the arrow keys and for Enter.
	if (dismissed) {
		return null;
	}

	return (
		<LexicalTypeaheadMenuPlugin<Choice>
			options={options}
			triggerFn={trigger}
			// The typed text is read above instead, which works whether or not
			// the menu is on screen.
			onQueryChange={() => {}}
			anchorClassName="slash__anchor"
			onSelectOption={(option, slash, close) => {
				editor.update(() => {
					// One update, so a single undo takes back both the slash
					// and what it turned the block into.
					slash?.remove();
					option.action.run(editor);
					close();
				});
			}}
			menuRenderFn={(
				anchor,
				{ selectedIndex, selectOptionAndCleanUp, setHighlightedIndex },
			) =>
				anchor.current === null
					? null
					: createPortal(
							<div
								ref={menu}
								className="slash"
								role="presentation"
								onMouseDown={(event) => event.preventDefault()}
							>
								{options.map((option, index) => (
									<button
										key={option.key}
										type="button"
										role="option"
										aria-selected={index === selectedIndex}
										ref={option.setRefElement}
										className="slash__option"
										onMouseEnter={() =>
											setHighlightedIndex(index)
										}
										onClick={() =>
											selectOptionAndCleanUp(option)
										}
									>
										{option.action.label}
										<span className="controls__keys">
											{option.action.keys
												? shortcutLabel(
														option.action.keys,
													)
												: option.action.hint}
										</span>
									</button>
								))}
								{hinted && (
									<div className="slash__hint">
										Type a name to find characters and
										locations
									</div>
								)}
							</div>,
							anchor.current,
						)
			}
		/>
	);
}
