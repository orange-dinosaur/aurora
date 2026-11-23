import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { COMMAND_PRIORITY_NORMAL, KEY_DOWN_COMMAND } from "lexical";
import { useEffect, useRef } from "react";
import {
	ACTIONS,
	FIND,
	OUTLINE,
	pressed,
	REPLACE,
	SCRIPT_MARKS,
	TOOLBAR,
} from "./formatting";

type Props = {
	/** A screenplay scene, which answers only to its three marks. */
	script: boolean;
	onToolbar: () => void;
	onOutline: () => void;
	onFind: () => void;
	onReplace: () => void;
};

// Ctrl+U is one of three shortcuts Lexical answers for itself. The other two
// are bold and italic, which Aurora wants; this one makes an underline, and
// markdown has nowhere to keep it — the writer would see it until the next time
// the file was opened.
const UNDERLINE = { key: "u", shift: false };

// The keyboard reaches the same actions as the two bars, so a shortcut cannot
// come to mean something the buttons do not do. Registered on the editor rather
// than on a bar, so hiding the toolbar does not take the keys with it.
export default function Shortcuts({
	script,
	onToolbar,
	onOutline,
	onFind,
	onReplace,
}: Props) {
	const [editor] = useLexicalComposerContext();
	// The callbacks change with the preferences, and the handler is not
	// registered again each time they do.
	const held = useRef({ onToolbar, onOutline, onFind, onReplace });
	useEffect(() => {
		held.current = { onToolbar, onOutline, onFind, onReplace };
	});

	useEffect(
		() =>
			editor.registerCommand(
				KEY_DOWN_COMMAND,
				(event) => {
					if (!script && pressed(event, UNDERLINE)) {
						event.preventDefault();
						return true;
					}

					if (pressed(event, TOOLBAR)) {
						event.preventDefault();
						held.current.onToolbar();
						return true;
					}

					if (pressed(event, OUTLINE)) {
						event.preventDefault();
						held.current.onOutline();
						return true;
					}

					if (pressed(event, FIND)) {
						event.preventDefault();
						held.current.onFind();
						return true;
					}

					if (pressed(event, REPLACE)) {
						event.preventDefault();
						held.current.onReplace();
						return true;
					}

					const action = (script ? SCRIPT_MARKS : ACTIONS).find(
						(each) => pressed(event, each.keys),
					);
					if (action === undefined) {
						return false;
					}
					// The webview binds some of these itself; taking the event
					// is what stops it acting on them too.
					event.preventDefault();
					action.run(editor);
					return true;
				},
				// Above the editor's own handler, which is where the built-in
				// bold, italic and underline live.
				COMMAND_PRIORITY_NORMAL,
			),
		[editor, script],
	);

	return null;
}
