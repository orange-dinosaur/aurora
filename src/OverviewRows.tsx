// What the tag and subject pages share: a row that opens a document, and how to
// find the document a row stands for.

import { useCallback } from "react";
import type { Corpus } from "./corpus";
import type { ProjectDocument } from "./types";

/** One document in a list on an overview page, with a count when it has one. */
export function OverviewRow({
	title,
	where,
	count,
	onOpen,
}: {
	title: string;
	where: string;
	count?: number;
	onOpen: () => void;
}) {
	return (
		<li>
			<button type="button" className="overview__row" onClick={onOpen}>
				<span className="overview__row-text">
					<span className="overview__row-title">{title}</span>
					<span className="overview__row-where">{where}</span>
				</span>
				{count !== undefined && (
					<span className="overview__row-count">{count}</span>
				)}
			</button>
		</li>
	);
}

/**
 * Looks a document up by id. A row on screen came out of the corpus, so its
 * document is in hand.
 */
export function useKnown(
	corpus: Corpus,
): (id: string) => ProjectDocument | undefined {
	return useCallback(
		(id) => (corpus.kind === "ready" ? corpus.known.get(id) : undefined),
		[corpus],
	);
}
