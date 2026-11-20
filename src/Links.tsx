import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CLICK_COMMAND, COMMAND_PRIORITY_LOW } from "lexical";
import { useEffect } from "react";
import { failure } from "./errors";
import { openable } from "./formatting";

// A plain click puts the caret in the words, as it does everywhere else in the
// text; holding the command key follows the link instead.
export default function Links({
	onFailed,
}: {
	/** Told why the desktop would not open an address. */
	onFailed: (message: string) => void;
}) {
	const [editor] = useLexicalComposerContext();

	useEffect(
		() =>
			editor.registerCommand(
				CLICK_COMMAND,
				(event) => {
					if (!event.ctrlKey && !event.metaKey) {
						return false;
					}
					const at = event.target;
					const link =
						at instanceof Element ? at.closest("a[href]") : null;
					const url = link?.getAttribute("href") ?? "";
					// Asking first, because the desktop is only ever handed a
					// web address: anything else stays a click in the text.
					if (!openable(url)) {
						return false;
					}
					event.preventDefault();
					openUrl(url).catch((failed: unknown) =>
						onFailed(failure(failed).message),
					);
					return true;
				},
				COMMAND_PRIORITY_LOW,
			),
		[editor, onFailed],
	);

	return null;
}
