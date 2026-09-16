import { useMemo, useState } from "react";
import { contextOf } from "./context";
import { useCorpus } from "./corpus";
import type { Seed } from "./lib/find";
import Icon from "./Icon";
import {
	appearancesIn,
	castOf,
	mentionsIn,
	RECOGNITION,
	subjectsIn,
	under,
	type Subject,
} from "./lib/mentions";
import { Line, toggled } from "./Search";
import { isSubject } from "./subjects";
import type { ProjectDocument } from "./types";

// Recognition read from whichever end the writer is standing at. On a
// character's page it is the search results list turned around: rather than
// starting from a word that was typed, it starts from the names the page
// answers to and sweeps the project for them. On a chapter it is the cast:
// which of those pages turn up in what is open, and how much of each. On a
// folder it is the cast of everything underneath, counted by document.

/** How many of a document's mentions are drawn before the rest are held back. */
const CAP = 20;

/** The document the panel is about, whether or not it is a subject's page. */
export type Page = Subject & { trail: string[] };

/** What the panel is about: an open document, or a folder's whole subtree. */
export type About =
	{ kind: "document"; page: Page } | { kind: "folder"; trail: string[] };

type Props = {
	root: string;
	about: About;
	/** Bumped whenever the project changes on disk. */
	changed: number;
	/** The text of every open document as its tab holds it, by id. */
	live: Map<string, string>;
	onOpen: (document: ProjectDocument, seed: Seed | null) => void;
};

/** One list of subjects, whichever end of recognition it came from. */
type Row = {
	id: string;
	title: string;
	/** Whether the document tags this subject as well as naming it. */
	tagged: boolean;
	/** Not drawn when it is nought, which is a subject that is only tagged. */
	count: number;
};

function Cast({ rows, onOpen }: { rows: Row[]; onOpen: (id: string) => void }) {
	return (
		<div className="search search--panel">
			<ul className="search__groups">
				{rows.map((row) => (
					<li key={row.id}>
						<button
							type="button"
							className="search__document"
							onClick={() => onOpen(row.id)}
						>
							<span className="search__title">{row.title}</span>
							<span className="search__aside">
								{row.tagged && (
									<span className="search__kind">Tagged</span>
								)}
								{row.count > 0 && (
									<span className="search__count">
										{row.count}
									</span>
								)}
							</span>
						</button>
					</li>
				))}
			</ul>
		</div>
	);
}

export default function Mentions({
	root,
	about,
	changed,
	live,
	onOpen,
}: Props) {
	const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
	const [full, setFull] = useState<Set<string>>(new Set());

	const { corpus, documents } = useCorpus({
		root,
		moved: changed,
		live,
		wanted: true,
	});

	// What the panel is about arrives freshly built on every render of the
	// project, so what recognition is run against is its content and not its
	// identity. Sweeping the whole project again because a keystroke landed
	// somewhere else would be the most expensive thing this panel does, so the
	// lists travel as text and are taken apart again inside the memo.
	const folder = about.kind === "folder";
	const id = about.kind === "document" ? about.page.id : "";
	const title = about.kind === "document" ? about.page.title : "";
	const own = about.kind === "document" && isSubject(about.page.trail);
	const held = (
		about.kind === "folder" ? about.trail : about.page.names
	).join("\n");

	const results = useMemo(() => {
		if (corpus.kind !== "ready") {
			return null;
		}

		const parts = held === "" ? [] : held.split("\n");

		if (folder) {
			return {
				kind: "folder" as const,
				cast: castOf(under(documents, parts), subjectsIn(documents)),
			};
		}

		if (own) {
			return {
				kind: "subject" as const,
				...mentionsIn({ id, title, names: parts }, documents),
			};
		}

		// The document has to be found in the corpus rather than parsed again:
		// what is on screen is the version its tab holds, which the corpus has
		// already merged in.
		const here = documents.find((document) => document.id === id);

		return {
			kind: "document" as const,
			appearances:
				here === undefined
					? []
					: appearancesIn(here, subjectsIn(documents)),
		};
	}, [corpus.kind, documents, folder, id, title, own, held]);

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

	if (results.kind === "folder") {
		if (results.cast.length === 0) {
			return (
				<p className="right-sidebar__empty">
					No character or place is named in here yet.
				</p>
			);
		}

		return (
			<>
				{/* The number means something else here than it does on a
				    document, so the panel says which. */}
				<p className="search__note">Documents naming each of them.</p>
				<Cast
					rows={results.cast.map(({ subject, count }) => ({
						id: subject.id,
						title: subject.title,
						tagged: false,
						count,
					}))}
					onOpen={(id) => openTab(id, null)}
				/>
			</>
		);
	}

	if (results.kind === "document") {
		if (results.appearances.length === 0) {
			return (
				<p className="right-sidebar__empty">
					No character or place is named here yet.
				</p>
			);
		}

		return (
			<Cast
				rows={results.appearances.map(({ subject, count, tagged }) => ({
					id: subject.id,
					title: subject.title,
					tagged,
					count,
				}))}
				onOpen={(id) => openTab(id, null)}
			/>
		);
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
