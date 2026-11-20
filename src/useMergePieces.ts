// Each conflicted document of a merge, in order, read once for every screen
// that shows the merge.

import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { failure } from "./errors";
import type { Merging, Piece } from "./types";

/** The pieces by document id, and why they could not be read, if they could not. */
export function useMergePieces(root: string, merging: Merging | null) {
	const [pieces, setPieces] = useState<Record<string, Piece[]>>({});
	const [failed, setFailed] = useState<string | null>(null);

	useEffect(() => {
		if (merging === null) {
			setPieces({});
			setFailed(null);
			return;
		}
		let live = true;
		const ids = merging.conflicts.flatMap((conflict) =>
			conflict.kind === "text" && conflict.id !== null
				? [conflict.id]
				: [],
		);
		Promise.all(
			ids.map(async (id) => {
				const found = await invoke<Piece[] | null>("merge_spots", {
					root,
					id,
				});
				return [id, found ?? []] as const;
			}),
		)
			.then((found) => {
				if (live) {
					setPieces(Object.fromEntries(found));
					setFailed(null);
				}
			})
			.catch((reason: unknown) => {
				if (live) {
					setFailed(failure(reason).message);
				}
			});
		return () => {
			live = false;
		};
	}, [root, merging]);

	return { pieces, failed };
}
