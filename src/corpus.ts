// The whole project, read once and matched against many times. Search and the
// Mentions panel both need every document opened and parsed, which is the
// expensive part of both, so the reading, the re-reading when the project moves
// on, and the open tabs beating the files they came from all live here rather
// than in either panel.

import {
	useCallback,
	useEffect,
	useMemo,
	useRef,
	useState,
	useSyncExternalStore,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { failure } from "./errors";
import { list, parse, split } from "./frontmatter";
import type { Mentionable } from "./mentions";
import { runsOf } from "./runs";
import { timed } from "./timing";
import type { DocumentText, ProjectDocument } from "./types";

export type Corpus =
	| { kind: "unread" }
	| { kind: "reading" }
	| {
			kind: "ready";
			/** The value of `changed` this was read at, which is how it knows it is old. */
			at: number;
			documents: Mentionable[];
			/**
			 * The documents themselves, by id. A result row knows an id; what
			 * it takes to open a tab is the whole document, and this is where
			 * the read that found it left one.
			 */
			known: Map<string, ProjectDocument>;
	  }
	| { kind: "failed"; message: string };

/**
 * How long the writer has to settle before the project is read. Only the first
 * read waits: typing `Wren` should sweep once, not four times.
 */
const SWEEP_MS = 250;

/** A document's text as both halves of recognition want it. */
function readIn(text: string | null): {
	runs: Mentionable["runs"];
	tags: string[];
	names: string[];
} {
	if (text === null) {
		return { runs: null, tags: [], names: [] };
	}

	const fields = parse(split(text).block);

	return {
		runs: runsOf(text),
		tags: list(fields, "tags"),
		names: list(fields, "names"),
	};
}

/** Reads every document in the project and parses it, or says why it could not. */
async function sweep(root: string, at: number): Promise<Corpus> {
	try {
		const documents = await timed(
			"corpus sweep",
			invoke<DocumentText[]>("read_all_documents", { root }),
		);

		return {
			kind: "ready",
			at,
			documents: documents.map(({ id, title, trail, text }) => ({
				id,
				title,
				trail,
				...readIn(text),
			})),
			known: new Map(
				documents.map(({ text: _text, ...document }) => [
					document.id,
					document,
				]),
			),
		};
	} catch (error) {
		return { kind: "failed", message: failure(error).message };
	}
}

/**
 * One reading of the project, shared by everything that wants it. Panels come
 * and go; the corpus outlives them, so opening Search, then Mentions, then a
 * subject page reads the project once instead of three times, and closing a
 * panel and opening it again reads nothing at all.
 *
 * Keyed by the project root and thrown away when that changes, which is the
 * one time a held corpus is worthless rather than merely old.
 */
type Store = {
	root: string;
	corpus: Corpus;
	/** The sweep in flight, so several panels asking at once still read once. */
	sweeping: Promise<void> | null;
	/** Told whenever `corpus` is replaced. */
	listeners: Set<() => void>;
};

let store: Store | null = null;

function storeFor(root: string): Store {
	if (store === null || store.root !== root) {
		store = {
			root,
			corpus: { kind: "unread" },
			sweeping: null,
			listeners: new Set(),
		};
	}

	return store;
}

function put(held: Store, corpus: Corpus): void {
	held.corpus = corpus;

	for (const listener of held.listeners) {
		listener();
	}
}

/**
 * Reads the project, unless it is already read at this `changed`, is already
 * being read, or failed and has not been asked again.
 */
function ask(held: Store, changed: number): void {
	if (
		held.sweeping !== null ||
		held.corpus.kind === "failed" ||
		(held.corpus.kind === "ready" && held.corpus.at === changed)
	) {
		return;
	}

	// A first read says so, so a panel can show it is working. A re-read
	// leaves what is drawn alone: they came to read it, not to watch it go.
	if (held.corpus.kind === "unread") {
		put(held, { kind: "reading" });
	}

	held.sweeping = sweep(held.root, changed).then((next) => {
		held.sweeping = null;

		// The project moved on under this read. Whatever it found describes a
		// project nobody is looking at any more.
		if (store === held) {
			put(held, next);
		}
	});
}

type Asked = {
	root: string;
	/**
	 * Counts the times the project has changed under what was read: a document
	 * written to disk, or the manifest itself changing.
	 */
	changed: number;
	/**
	 * The text of every open document as its tab holds it, by id. Newer than
	 * the file for the 800 ms after a keystroke, and newer than anything the
	 * sweep read for as long as the tab is open.
	 */
	live: Map<string, string>;
	/** Whether the corpus is wanted now. Nothing is read until it is. */
	wanted: boolean;
	/** Bumped to have another go after a read failed. */
	retry?: number;
};

/**
 * The project as search and recognition read it, with the documents already
 * merged with whatever their open tabs hold.
 */
export function useCorpus({ root, changed, live, wanted, retry = 0 }: Asked): {
	corpus: Corpus;
	documents: Mentionable[];
} {
	const held = storeFor(root);
	const subscribe = useCallback(
		(listener: () => void) => {
			held.listeners.add(listener);

			return () => {
				held.listeners.delete(listener);
			};
		},
		[held],
	);
	const corpus = useSyncExternalStore(subscribe, () => held.corpus);
	/** What the open tabs hold, which beats what was read from disk. */
	const [open, setOpen] = useState<Map<string, Mentionable["runs"]>>(
		new Map(),
	);

	// A sweep that failed is not retried on its own. Asking again is asking for
	// another go at it, and it is asked of the store, so every panel sitting on
	// the same failure recovers together.
	const asked = useRef(retry);
	useEffect(() => {
		if (asked.current === retry) {
			return;
		}

		asked.current = retry;

		if (held.corpus.kind === "failed") {
			put(held, { kind: "unread" });
		}
	}, [held, retry]);

	// Nothing is read until something wants it. Only the first read waits for
	// the writer to settle; a re-read, of a corpus the project has moved on
	// from, is already late and goes at once.
	useEffect(() => {
		if (!wanted) {
			return;
		}

		if (corpus.kind !== "unread") {
			ask(held, changed);
			return;
		}

		const timer = window.setTimeout(() => ask(held, changed), SWEEP_MS);

		return () => window.clearTimeout(timer);
	}, [held, wanted, corpus, changed]);

	// What the open tabs hold, parsed once the writer stops typing. A tab
	// changes on every keystroke, and parsing all of them on each one — with
	// whatever reads this then running again over the whole project — is the
	// one thing here that could be felt in the caret.
	useEffect(() => {
		if (!wanted) {
			return;
		}

		const timer = window.setTimeout(() => {
			setOpen(
				new Map(
					[...live].map(([id, text]) => [id, runsOf(text)] as const),
				),
			);
		}, SWEEP_MS);

		return () => window.clearTimeout(timer);
	}, [wanted, live]);

	// An open tab beats the file it came from: a word typed a moment ago is not
	// on disk yet, and search would otherwise say it is not there. Tags are
	// left as the file has them, since a tag is committed by the field it is
	// typed in rather than by every keystroke.
	const documents = useMemo(() => {
		if (corpus.kind !== "ready") {
			return [];
		}

		return corpus.documents.map((document) => {
			const runs = open.get(document.id);
			return runs === undefined ? document : { ...document, runs };
		});
	}, [corpus, open]);

	return { corpus, documents };
}
