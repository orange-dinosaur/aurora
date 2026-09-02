import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { contextOf, type Context } from "./context";
import { failure } from "./errors";
import Icon from "./Icon";
import { runsOf } from "./runs";
import { MIN_QUERY, search, type Searchable } from "./search";
import type { DocumentText } from "./types";

type Props = {
	root: string;
	/** Counts the times search has been asked for, so a second ask can answer. */
	asked: number;
};

/**
 * The project as search reads it. Reading it is the expensive part — every
 * document is opened and parsed — so it happens once, on the first query, and
 * every keystroke after that matches against what is already here.
 */
type Corpus =
	| { kind: "unread" }
	| { kind: "reading" }
	| { kind: "ready"; documents: Searchable[] }
	| { kind: "failed"; message: string };

/**
 * How long the writer has to stop typing before the project is read. Only the
 * first query waits: typing `Wren` should sweep once, not four times.
 */
const SWEEP_MS = 250;

/**
 * How many of a document's hits are drawn before the rest are held back. A
 * common word matches hundreds of times in a chapter, and every row here is a
 * real DOM node.
 */
const CAP = 20;

/**
 * What the writer has opened and closed. The choices are made against one
 * query and lapse when it changes: a group collapsed while looking for `wren`
 * says nothing about the same document under `window`.
 */
type Opened = {
	query: string;
	/** Documents collapsed to their header. A group is open by default. */
	collapsed: Set<string>;
	/** Documents showing every hit rather than the first `CAP`. */
	full: Set<string>;
};

const FRESH: Opened = { query: "", collapsed: new Set(), full: new Set() };

/** The set with `id` taken out if it was in it, and put in if it was not. */
function toggled(ids: Set<string>, id: string): Set<string> {
	const next = new Set(ids);
	if (!next.delete(id)) {
		next.add(id);
	}

	return next;
}

/**
 * A document's name with the query picked out of it. Search has already said
 * the name matches, so the query is in there; if it somehow is not, the name
 * is shown plain rather than nothing at all.
 */
function inTitle(title: string, query: string): Context {
	const at = title.toLowerCase().indexOf(query.toLowerCase());

	return at === -1
		? { before: title, match: "", after: "" }
		: contextOf(title, at, at + query.length);
}

/** One line of a document, with the match in it emphasised. */
function Line({ before, match, after }: Context) {
	return (
		<>
			{before}
			<mark className="search__match">{match}</mark>
			{after}
		</>
	);
}

export default function Search({ root, asked }: Props) {
	const [query, setQuery] = useState("");
	const [corpus, setCorpus] = useState<Corpus>({ kind: "unread" });
	const [opened, setOpened] = useState<Opened>(FRESH);
	const field = useRef<HTMLInputElement>(null);

	// Asking again puts the caret back in the field with the last query
	// selected, so a second Ctrl+Shift+F types over what is there rather than
	// leaving the writer to clear it.
	useEffect(() => {
		field.current?.focus();
		field.current?.select();
	}, [asked]);

	useEffect(() => {
		if (query.length < MIN_QUERY || corpus.kind !== "unread") {
			return;
		}

		const timer = window.setTimeout(() => {
			setCorpus({ kind: "reading" });

			invoke<DocumentText[]>("read_all_documents", { root })
				.then((documents) => {
					setCorpus({
						kind: "ready",
						documents: documents.map(
							({ id, title, folder, text }) => ({
								id,
								title,
								folder,
								runs: text === null ? null : runsOf(text),
							}),
						),
					});
				})
				.catch((error: unknown) => {
					setCorpus({
						kind: "failed",
						message: failure(error).message,
					});
				});
		}, SWEEP_MS);

		return () => window.clearTimeout(timer);
	}, [query, corpus.kind, root]);

	const results = useMemo(
		() =>
			corpus.kind === "ready" ? search(corpus.documents, query) : null,
		[corpus, query],
	);

	// Choices made against an older query are not choices about these results.
	const view = opened.query === query ? opened : FRESH;

	function collapse(id: string) {
		setOpened({ ...view, query, collapsed: toggled(view.collapsed, id) });
	}

	function showAll(id: string) {
		setOpened({ ...view, query, full: new Set(view.full).add(id) });
	}

	return (
		<section className="search">
			<input
				ref={field}
				type="search"
				className="search__field"
				placeholder="Search this project"
				aria-label="Search this project"
				value={query}
				onChange={(event) => setQuery(event.target.value)}
			/>

			{query.length < MIN_QUERY ? (
				<p className="search__note">
					{query === ""
						? "Every document in the project, by what is written in it or what it is called."
						: "Keep typing."}
				</p>
			) : corpus.kind === "failed" ? (
				<p className="search__note search__note--error">
					{corpus.message}
				</p>
			) : results === null ? (
				<p className="search__note">Reading the project…</p>
			) : results.groups.length === 0 &&
			  results.unreadable.length === 0 ? (
				<p className="search__note">Nothing found.</p>
			) : (
				<div className="search__results">
					<ul className="search__groups">
						{results.groups.map((group) => {
							const open = !view.collapsed.has(group.id);
							const capped =
								!view.full.has(group.id) &&
								group.hits.length > CAP;

							return (
								<li key={group.id} className="search__group">
									<button
										type="button"
										className="search__document"
										aria-expanded={open}
										onClick={() => collapse(group.id)}
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
										<span className="search__folder">
											{group.folder}
										</span>
										<span className="search__count">
											{group.count}
										</span>
									</button>

									{open && (
										<ul className="search__hits">
											{group.titleHit && (
												<li className="search__hit search__hit--title">
													<span className="search__kind">
														Title
													</span>
													<span className="search__line">
														<Line
															{...inTitle(
																group.title,
																query,
															)}
														/>
													</span>
												</li>
											)}

											{(capped
												? group.hits.slice(0, CAP)
												: group.hits
											).map((hit) => (
												<li
													key={hit.ordinal}
													className="search__hit"
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
												</li>
											))}

											{capped && (
												<li className="search__hit">
													<button
														type="button"
														className="search__more"
														onClick={() =>
															showAll(group.id)
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
						<div className="search__unreadable">
							<p className="search__note">
								{results.unreadable.length === 1
									? "One document could not be read."
									: `${results.unreadable.length} documents could not be read.`}
							</p>
							<ul className="search__unread">
								{results.unreadable.map((document) => (
									<li
										key={document.id}
										className="search__document search__document--plain"
									>
										<span className="search__title">
											{document.title}
										</span>
										<span className="search__folder">
											{document.folder}
										</span>
									</li>
								))}
							</ul>
						</div>
					)}
				</div>
			)}
		</section>
	);
}
