// The open document's front matter, as fields, from inside the editor. The
// block is part of the Lexical root state, so reading it takes an editor state
// and writing it is an ordinary edit: the change dirties the document and the
// autosave already in place carries it to the file. Nothing here writes to
// disk itself.

import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { useCallback, useEffect, useMemo, useState } from "react";
import { parse, serialize, type Fields, type Value } from "./frontmatter";
import { $frontMatter, $setFrontMatter } from "./markdown";

/** Setting a field, or dropping it when the value is null. */
export type SetField = (key: string, value: Value | null) => void;

/**
 * What the panel beside the writing needs from the editor: the open document's
 * fields, and a way to change one.
 */
export type FieldsHandle = { fields: Fields; setField: SetField };

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
export function useFields(): FieldsHandle {
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

	const setField = useCallback<SetField>(
		(key, value) => {
			editor.update(() => $setField(key, value));
		},
		[editor],
	);

	// Memoised as one object: whatever holds it compares it by identity, and a
	// fresh one on every render would have the panel rebuilding as fast as the
	// writer types.
	return useMemo(
		() => ({ fields: parse(block), setField }),
		[block, setField],
	);
}
