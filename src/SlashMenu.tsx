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
import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { shortcutLabel, type Action } from "./formatting";
import { $slashAt, matching } from "./slash";

// The menu is handed options rather than actions, so each one carries the
// action it stands for.
class Choice extends MenuOption {
	readonly action: Action;

	constructor(action: Action) {
		super(action.id);
		this.action = action;
	}
}

// Nothing to match means no menu at all, rather than an empty one: an open
// menu takes the arrow keys, and the caret would stop moving.
function trigger(_text: string, editor: LexicalEditor): MenuTextMatch | null {
	const query = editor.getEditorState().read($slashAt);
	if (query === null || matching(query).length === 0) {
		return null;
	}
	return {
		leadOffset: 0,
		matchingString: query,
		replaceableString: `/${query}`,
	};
}

// A slash at the start of an empty block offers the same blocks the bar's
// dropdown does, without the writer having to reach for it.
export default function SlashMenu() {
	const [editor] = useLexicalComposerContext();
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

	const options = useMemo(
		() => matching(typed ?? "").map((action) => new Choice(action)),
		[typed],
	);

	const showing = typed !== null && !dismissed && options.length > 0;

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
											{shortcutLabel(option.action.keys)}
										</span>
									</button>
								))}
							</div>,
							anchor.current,
						)
			}
		/>
	);
}
