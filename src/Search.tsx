import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { contextOf, type Context } from "./context";
import { failure } from "./errors";
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
			) : results.groups.length === 0 ? (
				<p className="search__note">Nothing found.</p>
			) : (
				<ul className="search__groups">
					{results.groups.map((group) => (
						<li key={group.id} className="search__group">
							<p className="search__document">
								<span className="search__title">
									{group.title}
								</span>
								<span className="search__folder">
									{group.folder}
								</span>
								<span className="search__count">
									{group.count}
								</span>
							</p>

							<ul className="search__hits">
								{group.titleHit && (
									<li className="search__hit search__hit--title">
										<span className="search__kind">
											Title
										</span>
										<span className="search__line">
											<Line
												{...inTitle(group.title, query)}
											/>
										</span>
									</li>
								)}

								{group.hits.map((hit) => (
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
							</ul>
						</li>
					))}
				</ul>
			)}
		</section>
	);
}
