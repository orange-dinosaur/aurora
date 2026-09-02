import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
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
						</li>
					))}
				</ul>
			)}
		</section>
	);
}
