// Following a renamed document with the tags that spell its old title: the
// offer, and the rewrite of every document wearing one.

import { invoke } from "@tauri-apps/api/core";
import { useState, type Dispatch, type SetStateAction } from "react";
import { refile } from "./corpus";
import { readDocument, writeDocument } from "./documents";
import { failure } from "./errors";
import { rekey, type Sides } from "./split";
import { retag, tagged } from "./tags";
import type { DocumentTab } from "./tabbing";
import { timed } from "./timing";
import type { DocumentText } from "./types";

/**
 * What is being offered after a document was renamed out from under the tags
 * pointing at it. `from` and `to` are the titles; `ids` are the documents
 * wearing the old one.
 */
export type Retagging =
	| { kind: "no" }
	| { kind: "asking"; from: string; to: string; ids: string[] }
	| { kind: "writing"; from: string; to: string; ids: string[] }
	| { kind: "failed"; message: string };

/** What the rewrite needs from the open tabs. */
type Tabs = {
	documentTab: (id: string) => DocumentTab | undefined;
	stopTimer: (id: string) => void;
	save: (id: string, text: string, checked: boolean) => Promise<void>;
	patch: (id: string, change: (tab: DocumentTab) => DocumentTab) => void;
	freshKey: () => string;
	setOnPage: Dispatch<SetStateAction<Sides>>;
	/** Told once a run has finished, so the lists read the project again. */
	written: () => void;
};

export function useRetagging(root: string, tabs: Tabs) {
	const [retagging, setRetagging] = useState<Retagging>({ kind: "no" });

	/**
	 * Whether anything was filed under the title this document has just left.
	 * A tag is the writer's word and not a reference Aurora maintains, so the
	 * rename lands either way and this only offers to follow it.
	 */
	async function countTags(from: string, to: string) {
		try {
			const all = await timed(
				"retag scan",
				invoke<DocumentText[]>("read_all_documents", { root }),
			);
			const ids = all.flatMap(({ id, text }) =>
				text !== null && tagged(text, from) ? [id] : [],
			);

			if (ids.length > 0) {
				setRetagging({ kind: "asking", from, to, ids });
			}
		} catch {
			// The rename itself landed. Nothing is offered if the rest of the
			// project could not be read, and nothing is said about it either:
			// a warning about a question that was never asked is noise.
		}
	}

	/**
	 * Rewrites one document's tag. A document with a tab open is written from
	 * what the tab holds rather than from the file, which may be up to 800 ms
	 * behind it, and then comes back under a fresh key: an editor seeds itself
	 * from its text once and owns it from there, so the only way to show it the
	 * new tag is to mount it again. That costs the tab its undo history, which
	 * is why nothing here touches a document that is merely open.
	 */
	async function retagOne(id: string, from: string, to: string) {
		const tab = tabs.documentTab(id);

		if (tab === undefined || tab.content.kind !== "ready") {
			const text = retag(await readDocument(root, id), from, to);
			await writeDocument(root, id, text);
			return;
		}

		const text = retag(tab.content.text, from, to);
		tabs.stopTimer(id);
		await tabs.save(id, text, false);

		const key = tabs.freshKey();
		tabs.patch(id, (open) => ({
			...open,
			key,
			content: { kind: "ready", text },
			save: { kind: "clean" },
		}));
		tabs.setOnPage((now) => rekey(now, new Map([[tab.key, key]])));
	}

	// This is the one write that is not told to the corpus a document at a
	// time. Handing over hundreds of texts in a loop would redraw everything
	// looking at the corpus once per document, and recognition reads the whole
	// project each time it is redrawn. One read at the end is cheaper than the
	// patches, and retagging is rare enough to pay for it.
	async function retagAll(from: string, to: string, ids: string[]) {
		setRetagging({ kind: "writing", from, to, ids });
		try {
			for (const id of ids) {
				await retagOne(id, from, to);
			}
			setRetagging({ kind: "no" });
			tabs.written();
		} catch (error) {
			setRetagging({ kind: "failed", message: failure(error).message });
		} finally {
			// A run that failed part way still rewrote what it got to.
			refile(root);
		}
	}

	return {
		retagging,
		countTags,
		retagAll,
		dismiss: () => setRetagging({ kind: "no" }),
	};
}
