// The open document's front matter, as fields, from inside the editor. The
// block is part of the Lexical root state, so reading it takes an editor state
// and writing it is an ordinary edit: the change dirties the document and the
// autosave already in place carries it to the file. Nothing here writes to
// disk itself.

import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
	parse,
	pooled,
	serialize,
	type Fields,
	type Value,
} from "./frontmatter";
import { useCorpus } from "./corpus";
import { failure } from "./errors";
import { $frontMatter, $setFrontMatter } from "./markdown";
import { subjectNames } from "./subjects";
import { timed } from "./timing";
import type { TreeNode } from "./types";

/** Setting a field, or dropping it when the value is null. */
export type SetField = (key: string, value: Value | null) => void;

/**
 * What the panel beside the writing needs from the editor: the open document's
 * fields, and a way to change one.
 */
export type FieldsHandle = {
	fields: Fields;
	setField: SetField;
	/** Writes what is waiting now, for fields that do not ride on an autosave. */
	flush?: () => void;
};

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

/** One map, so a folder with nothing read yet does not render as a new one. */
const EMPTY: Fields = new Map();

/**
 * The same panel, about a folder. A folder has no file to keep a front matter
 * block in, so its fields live on its node in the manifest and a change here
 * goes straight to disk rather than riding on a document's autosave. A write
 * that is refused puts the field back to what it was and says why.
 */
export function useFolderFields(
	root: string,
	id: string | null,
	/** Bumped whenever the manifest changes, so the fields are read again. */
	changed: number,
): { handle: FieldsHandle | null; trouble: string | null } {
	// The fields travel with the folder they were read for, so moving to
	// another one shows nothing rather than the last one's synopsis while the
	// read is in flight.
	const [read, setRead] = useState<{ id: string; fields: Fields } | null>(
		null,
	);
	const [trouble, setTrouble] = useState<string | null>(null);

	const fields = read !== null && read.id === id ? read.fields : EMPTY;

	// What a write starts from, without making every write a new callback.
	const held = useRef(fields);
	held.current = fields;

	// Typing makes many changes and the manifest takes the whole map each time,
	// so a change waits for a pause. `before` is what the folder held when the
	// first unsent change was made, which is what a refusal puts back.
	const unsent = useRef<{
		root: string;
		id: string;
		fields: Fields;
		before: Fields;
	} | null>(null);
	const timer = useRef<number | null>(null);
	// Counts the changes, so only the newest can roll back and a read that
	// began before one is not allowed to overwrite it.
	const writes = useRef(0);

	useEffect(() => {
		if (id === null) {
			return;
		}

		let wanted = true;
		const began = writes.current;
		void invoke<Record<string, Value>>("folder_fields", { root, id })
			.then((said) => {
				if (wanted && began === writes.current) {
					setRead({ id, fields: new Map(Object.entries(said)) });
					setTrouble(null);
				}
			})
			.catch((error: unknown) => {
				if (wanted) {
					// Not live again until a read lands: a write from the empty
					// map would replace every other field in the manifest.
					setRead(null);
					setTrouble(failure(error).message);
				}
			});

		// A read that comes back after the writer has moved on belongs to a
		// folder that is no longer the one on screen.
		return () => {
			wanted = false;
		};
	}, [root, id, changed]);

	const send = useCallback(() => {
		if (timer.current !== null) {
			window.clearTimeout(timer.current);
			timer.current = null;
		}
		const sending = unsent.current;
		unsent.current = null;
		if (sending === null) {
			return;
		}

		const mine = writes.current;
		void invoke("set_folder_fields", {
			root: sending.root,
			id: sending.id,
			fields: Object.fromEntries(sending.fields),
		})
			.then(() => setTrouble(null))
			.catch((error: unknown) => {
				if (mine === writes.current) {
					held.current = sending.before;
					setRead({ id: sending.id, fields: sending.before });
				}
				setTrouble(failure(error).message);
			});
	}, []);

	// Leaving the folder sends what is waiting rather than dropping it.
	useEffect(() => send, [root, id, send]);

	const setField = useCallback<SetField>(
		(key, value) => {
			if (id === null) {
				return;
			}

			const was = held.current;
			const next = new Map(was);
			if (value === null) {
				next.delete(key);
			} else {
				next.set(key, value);
			}
			held.current = next;
			setRead({ id, fields: next });

			writes.current += 1;
			unsent.current = {
				root,
				id,
				fields: next,
				before: unsent.current?.before ?? was,
			};
			if (timer.current !== null) {
				window.clearTimeout(timer.current);
			}
			timer.current = window.setTimeout(send, 800);
		},
		[root, id, send],
	);

	return {
		// Not until the read has landed, or a keystroke would write over the
		// folder's other fields with a map that has none of them.
		handle:
			id === null || read === null || read.id !== id
				? null
				: { fields, setField, flush: send },
		trouble,
	};
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

/** No open tabs to merge in: a name only counts once it has reached disk. */
const NO_TABS = new Map<string, string>();

/**
 * The field names already used somewhere in the project, for the Add field box
 * to suggest.
 *
 * They come from the shared reading of the project, which is only started when
 * the box is asked for rather than when the panel appears. Failing quietly is
 * deliberate: a suggestion is a convenience, and the writer can always type
 * the name out.
 */
export function useFieldNames(
	root: string,
	changed: number,
): { names: string[]; ask: () => void } {
	const [wanted, setWanted] = useState(false);
	const { corpus } = useCorpus({
		root,
		moved: changed,
		live: NO_TABS,
		wanted,
	});
	const names = useMemo(
		() => (corpus.kind === "ready" ? pooled(corpus.keys.values()) : []),
		[corpus],
	);
	const ask = useCallback(() => setWanted(true), []);

	return { names, ask };
}

/**
 * The subjects a relationship could point at, read on the same terms as the
 * field names above: only once the control is used, and again after the
 * project changes. The tree is enough for this, so no file is opened.
 */
export function useSubjectNames(
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

		void timed(
			"subject names",
			invoke<TreeNode[]>("document_tree", { root }),
		)
			.then((tree) => setNames(subjectNames(tree)))
			.catch(() => {});
	}, [root, changed]);

	return { names, ask };
}
