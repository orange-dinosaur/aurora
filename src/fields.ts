// The open document's front matter, as fields, from inside the editor. The
// block is part of the Lexical root state, so reading it takes an editor state
// and writing it is an ordinary edit: the change dirties the document and the
// autosave already in place carries it to the file. Nothing here writes to
// disk itself.

import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
	fieldNames,
	parse,
	serialize,
	type Fields,
	type Value,
} from "./frontmatter";
import { $frontMatter, $setFrontMatter } from "./markdown";
import type { DocumentText } from "./types";

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

/**
 * The field names already used somewhere in the project, for the Add field box
 * to suggest.
 *
 * Reading them opens every file, so it happens when the box is asked for rather
 * than when the panel appears, and the answer is kept until something reaches
 * disk. Failing quietly is deliberate: a suggestion is a convenience, and the
 * writer can always type the name out.
 */
export function useFieldNames(
	root: string,
	changed: number,
): { names: string[]; ask: () => void } {
	const [names, setNames] = useState<string[]>([]);
	const read = useRef(-1);

	const ask = useCallback(() => {
		if (read.current === changed) {
			return;
		}
		read.current = changed;

		void invoke<DocumentText[]>("read_all_documents", { root })
			.then((documents) =>
				setNames(
					fieldNames(
						documents.flatMap(({ text }) =>
							text === null ? [] : [text],
						),
					),
				),
			)
			.catch(() => {});
	}, [root, changed]);

	return { names, ask };
}
