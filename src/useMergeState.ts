// What the compare and merge screens share with the rest of a project: the
// merge waiting to be settled, what the writer has chosen in it, and the
// version Compare opened on.

import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { Choices, Sides } from "./lib/merge";
import type { MergeUnderway } from "./lib/versions";
import type { Branch, Merging } from "./types";
import { useMergePieces } from "./useMergePieces";

export function useMergeState(root: string) {
	// The version the compare view opened on. It takes the whole window and
	// reads now from disk, so every editor is written before it opens.
	// The checkpoint compare opens at, the document it opens on across the
	// whole project, and the version whose checkpoints it lists (null for the
	// one the writer is in).
	const [comparing, setComparing] = useState<{
		checkpoint: string;
		pick: string | null;
		slug: string | null;
	} | null>(null);
	// The merge waiting to be settled, which the titlebar and the Versions
	// tab both show.
	const [merge, setMerge] = useState<MergeUnderway | null>(null);
	// The document open on the merge screen, and what the writer has chosen
	// for each document's spots and the binder's cards. Choices last until
	// the merge ends.
	const [settling, setSettling] = useState<string | null>(null);
	const [choices, setChoices] = useState<Record<string, Choices>>({});
	const [sides, setSides] = useState<Sides>({});

	async function readMerge() {
		try {
			const merging = await invoke<Merging | null>("merge_state", {
				root,
			});
			if (merging === null) {
				setMerge(null);
				setSettling(null);
				setChoices({});
				setSides({});
				return;
			}
			const versions = await invoke<Branch[]>("list_branches", { root });
			const into = versions.find((version) => version.current);
			setMerge({ merging, into: into?.name ?? "" });
		} catch {
			// The Versions tab reads the same repository and says what is wrong.
		}
	}

	useEffect(() => {
		void readMerge();
	}, [root]);

	const { pieces, failed } = useMergePieces(root, merge?.merging ?? null);

	return {
		pieces,
		piecesFailed: failed,
		comparing,
		setComparing,
		merge,
		settling,
		setSettling,
		choices,
		setChoices,
		sides,
		setSides,
		readMerge,
	};
}
