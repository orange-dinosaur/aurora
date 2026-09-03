import { useMemo, useState } from "react";
import { contextOf } from "./context";
import { useCorpus } from "./corpus";
import type { Seed } from "./find";
import Icon from "./Icon";
import { mentionsIn, RECOGNITION, type Subject } from "./mentions";
import { Line, toggled } from "./Search";
import type { ProjectDocument } from "./types";

// Where in the project a character or a place is spoken of. It is the search
// results list read the other way round: search starts from a word the writer
// typed, this starts from the page they have open and looks for the names it
// answers to.

/** How many of a document's mentions are drawn before the rest are held back. */
const CAP = 20;

type Props = {
	root: string;
	/** The page this panel is about. */
	subject: Subject;
	/** Bumped whenever the project changes on disk. */
	changed: number;
	/** The text of every open document as its tab holds it, by id. */
	live: Map<string, string>;
	onOpen: (document: ProjectDocument, seed: Seed | null) => void;
};

export default function Mentions({
	root,
	subject,
	changed,
	live,
	onOpen,
}: Props) {
	const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
	const [full, setFull] = useState<Set<string>>(new Set());

	const { corpus, documents } = useCorpus({
		root,
		changed,
		live,
		wanted: true,
	});

	// The subject arrives freshly built on every render of the project, so what
	// recognition is run against is its content and not its identity. Sweeping
	// the whole project again because a keystroke landed somewhere else would
	// be the most expensive thing this panel does.
	const { id, title } = subject;
	const names = subject.names.join("\n");

	const results = useMemo(() => {
		if (corpus.kind !== "ready") {
			return null;
		}

		return mentionsIn(
			{ id, title, names: names === "" ? [] : names.split("\n") },
			documents,
		);
	}, [corpus.kind, documents, id, title, names]);

	// A row that is on screen came out of the corpus, so its document is in
	// hand; nothing happens if it somehow is not.
	function openTab(document: string, seed: Seed | null) {
		const known =
			corpus.kind === "ready" ? corpus.known.get(document) : undefined;
		if (known !== undefined) {
			onOpen(known, seed);
		}
	}

	if (corpus.kind === "failed") {
		return (
			<p className="search__note search__note--error">{corpus.message}</p>
		);
	}

	if (results === null) {
		return <p className="right-sidebar__empty">Reading the project…</p>;
	}

	if (results.groups.length === 0 && results.unreadable.length === 0) {
		return (
			<p className="right-sidebar__empty">
				Not named anywhere else in the project yet.
			</p>
		);
	}

	return (
		<div className="search search--panel">
			<div className="search__results">
				<ul className="search__groups">
					{results.groups.map((group) => {
						const open = !collapsed.has(group.id);
						const capped =
							!full.has(group.id) && group.hits.length > CAP;

						return (
							<li key={group.id} className="search__group">
								<button
									type="button"
									className="search__document"
									aria-expanded={open}
									onClick={() =>
										setCollapsed(
											toggled(collapsed, group.id),
										)
									}
								>
									<Icon
										name={
											open
												? "chevron-down"
												: "chevron-right"
										}
									/>
									<span className="search__title">
										{group.title}
									</span>
									<span className="search__count">
										{group.count}
									</span>
								</button>

								{open && (
									<ul className="search__hits">
										{group.tagged.map((tag) => (
											<li key={`tag:${tag}`}>
												{/* A tag is a claim about the
												    whole document, so there is
												    nowhere in it to land. */}
												<button
													type="button"
													className="search__hit search__hit--title"
													onClick={() =>
														openTab(group.id, null)
													}
												>
													<span className="search__kind">
														Tagged
													</span>
													<span className="search__line">
														{tag}
													</span>
												</button>
											</li>
										))}

										{(capped
											? group.hits.slice(0, CAP)
											: group.hits
										).map((hit) => (
											<li
												key={`${hit.name}:${hit.ordinal}`}
											>
												<button
													type="button"
													className="search__hit"
													onClick={() =>
														openTab(group.id, {
															query: hit.name,
															ordinal:
																hit.ordinal,
															reading:
																RECOGNITION,
														})
													}
												>
													<span className="search__line">
														<Line
															{...contextOf(
																hit.line,
																hit.from,
																hit.to,
															)}
														/>
													</span>
												</button>
											</li>
										))}

										{capped && (
											<li>
												<button
													type="button"
													className="search__more"
													onClick={() =>
														setFull(
															new Set(full).add(
																group.id,
															),
														)
													}
												>
													Show all {group.count}
												</button>
											</li>
										)}
									</ul>
								)}
							</li>
						);
					})}
				</ul>

				{results.unreadable.length > 0 && (
					<p className="search__note">
						{results.unreadable.length === 1
							? "One document could not be read."
							: `${results.unreadable.length} documents could not be read.`}
					</p>
				)}
			</div>
		</div>
	);
}
