import { useMemo } from "react";
import { useCorpus } from "./corpus";
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

	// A row on screen came out of the corpus, so its document is in hand.
	function open(id: string) {
		const known =
			corpus.kind === "ready" ? corpus.known.get(id) : undefined;
		if (known !== undefined) {
			onOpen(known);
		}
	}

	if (corpus.kind === "failed") {
		return (
			<section className="search">
				<p className="search__note search__note--error">
					{corpus.message}
				</p>
			</section>
		);
	}

	if (corpus.kind !== "ready") {
		return (
			<section className="search">
				<p className="search__note">Reading the project…</p>
			</section>
		);
	}

	return (
		<section className="search">
			<p className="search__note">
				{wearing.length === 0
					? "Nothing is tagged"
					: wearing.length === 1
						? "One document tagged"
						: `${wearing.length} documents tagged`}{" "}
				<span className="search__tag">{tag}</span>
			</p>

			{wearing.length > 0 && (
				<div className="search__results">
					<ul className="search__groups">
						{wearing.map((document) => (
							<li key={document.id}>
								<button
									type="button"
									className="search__document"
									onClick={() => open(document.id)}
								>
									<span className="search__title">
										{document.title}
									</span>
									<span className="search__folder">
										{document.trail.join(" / ")}
									</span>
								</button>
							</li>
						))}
					</ul>
				</div>
			)}
		</section>
	);
}
