import { useMemo } from "react";
import { useCorpus } from "./corpus";
import { OverviewRow, useKnown } from "./OverviewRows";
import type { ProjectDocument } from "./types";

// Everything wearing one tag. This is where a chip lands when it names no
// document of its own: `needs-work` is a keyword and has only a list behind it,
// while `Elena` is a page and goes straight there. The project view decides
// which of the two a chip is; by the time this tab exists the answer is a list.

type Props = {
	root: string;
	tag: string;
	/** Bumped whenever the project changes on disk. */
	changed: number;
	/** The text of every open document as its tab holds it, by id. */
	live: Map<string, string>;
	onOpen: (document: ProjectDocument) => void;
};

export default function TagView({ root, tag, changed, live, onOpen }: Props) {
	const { corpus, documents } = useCorpus({
		root,
		moved: changed,
		live,
		wanted: true,
	});

	// Case-sensitive, the same way recognition reads a name in prose: the case
	// the writer typed is the case that counts.
	const wearing = useMemo(
		() => documents.filter((document) => document.tags.includes(tag)),
		[documents, tag],
	);

	const known = useKnown(corpus);

	function open(id: string) {
		const found = known(id);
		if (found !== undefined) {
			onOpen(found);
		}
	}

	return (
		<section className="overview overview--sheet">
			<header className="overview__header">
				<div className="overview__heading">
					<h2 className="overview__title">{tag}</h2>
					{corpus.kind === "ready" ? (
						<p className="overview__count overview__count--chip">
							{wearing.length === 0
								? "Nothing is tagged"
								: wearing.length === 1
									? "One document tagged"
									: `${wearing.length} documents tagged`}
							<span className="overview__tag">{tag}</span>
						</p>
					) : corpus.kind !== "failed" ? (
						<p className="overview__count">Reading the project…</p>
					) : null}
				</div>
			</header>

			{corpus.kind === "failed" && (
				<p className="overview__message">{corpus.message}</p>
			)}

			{corpus.kind === "ready" && wearing.length > 0 && (
				<ul className="overview__rows">
					{wearing.map((document) => (
						<OverviewRow
							key={document.id}
							title={document.title}
							where={document.trail.join(" / ")}
							onOpen={() => open(document.id)}
						/>
					))}
				</ul>
			)}
		</section>
	);
}
