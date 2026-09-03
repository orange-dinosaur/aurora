// The open document's front matter, as fields, from inside the editor. The
// block is part of the Lexical root state, so reading it takes an editor state
// and writing it is an ordinary edit: the change dirties the document and the
// autosave already in place carries it to the file. Nothing here writes to
// disk itself.

import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { useCallback, useEffect, useMemo, useState } from "react";
import { parse, serialize, type Fields, type Value } from "./frontmatter";
import { $frontMatter, $setFrontMatter } from "./markdown";

/** The fields the document carries. Call inside a read of an editor state. */
export function $fields(): Fields {
	return parse($frontMatter());
}

/**
 * Sets one field, or drops it when the value is null. Call inside an update.
 * Everything else in the block, down to the spelling of the fields that did
 * not change, is left as the writer had it.
 */
export function $setField(key: string, value: Value | null): void {
	const fields = $fields();

	if (value === null) {
		fields.delete(key);
	} else {
		fields.set(key, value);
	}

	$setFrontMatter(serialize(fields, $frontMatter()));
}

/**
 * The fields of the document being edited, kept in step with it, and a way to
 * change one. Only works inside the editor's composer.
 */
export function useFields(): {
	fields: Fields;
	setField: (key: string, value: Value | null) => void;
} {
	const [editor] = useLexicalComposerContext();

	// The block is held as a string, so typing anywhere else in the document
	// hands back the same one and nothing reading fields has to render again.
	const [block, setBlock] = useState(() =>
		editor.getEditorState().read($frontMatter),
	);

	useEffect(
		() =>
			editor.registerUpdateListener(({ editorState }) => {
				setBlock(editorState.read($frontMatter));
			}),
		[editor],
	);

	const setField = useCallback(
		(key: string, value: Value | null) => {
			editor.update(() => $setField(key, value));
		},
		[editor],
	);

	return { fields: useMemo(() => parse(block), [block]), setField };
}
