import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useCorpus } from "./corpus";
import { failure } from "./errors";
import type { Seed } from "./find";
import {
	list,
	parse,
	serialize,
	split,
	text,
	ties,
	type Fields,
} from "./frontmatter";
import { mentionsIn, subjectsIn } from "./mentions";
import { factsOf, linksOf } from "./subjects";
import type { ProjectDocument } from "./types";

// A page about a person or a place. The same document the editor opens, laid
// out as what the writer knows about it: the fields they gave it, the remarks,
// and every scene it turns up in. Recognition is the same sweep the Mentions
// panel runs, so the two never disagree.
//
// One thing here writes, and that is the remarks box. The prose stays the
// editor's, and the file still has a single holder: when a tab has this
// document open the box goes through that tab and the tab's own autosave
// rather than writing behind it, and the tab is rebuilt from the new text so
// it cannot put stale front matter back later.

type Props = {
	root: string;
	/** The subject's own document, which is the page. */
	subject: ProjectDocument;
	/** Bumped whenever the project changes on disk. */
	changed: number;
	/** The text of every open document as its tab holds it, by id. */
	live: Map<string, string>;
	onOpen: (document: ProjectDocument, seed: Seed | null) => void;
	/** Opens the subject's own document, which is the way back to writing. */
	onDraft: () => void;
	/** Turns this page to another subject's, rather than opening a second one. */
	onSubject: (document: ProjectDocument) => void;
	/**
	 * Sets the remarks, by handing over the document's whole text with them
	 * already in it. Writing is on the same countdown the editor uses, so this
	 * is told separately how to say that the write failed.
	 */
	onRemarks: (text: string, failed: (message: string) => void) => void;
};

const EMPTY: Fields = new Map();

type Held =
	| { kind: "reading" }
	| { kind: "ready"; text: string }
	| { kind: "failed"; message: string };

export default function SubjectView({
	root,
	subject,
	changed,
	live,
	onOpen,
	onDraft,
	onSubject,
	onRemarks,
}: Props) {
	const [held, setHeld] = useState<Held>({ kind: "reading" });

	// What the tab holds beats what is on disk, the same way the corpus takes
	// an open document from its tab: a name typed into Info a second ago is
	// what this page should be about.
	const open = live.get(subject.id);

	useEffect(() => {
		if (open !== undefined) {
			setHeld({ kind: "ready", text: open });
			return;
		}

		let gone = false;
		void invoke<string>("read_document", { root, id: subject.id })
			.then((found) => {
				if (!gone) {
					setHeld({ kind: "ready", text: found });
				}
			})
			.catch((error: unknown) => {
				if (!gone) {
					setHeld({
						kind: "failed",
						message: failure(error).message,
					});
				}
			});

		return () => {
			gone = true;
		};
	}, [root, subject.id, open, changed]);

	const { corpus, documents } = useCorpus({
		root,
		changed,
		live,
		wanted: true,
	});

	const fields = useMemo(
		() => (held.kind === "ready" ? parse(split(held.text).block) : EMPTY),
		[held],
	);

	// The names travel as one string so a re-render that changed nothing about
	// the page does not sweep the project again.
	const heldNames = list(fields, "names").join("\n");

	const appearances = useMemo(() => {
		if (corpus.kind !== "ready") {
			return [];
		}

		return mentionsIn(
			{
				id: subject.id,
				title: subject.title,
				names: heldNames === "" ? [] : heldNames.split("\n"),
			},
			documents,
		).groups;
	}, [corpus.kind, documents, subject.id, subject.title, heldNames]);

	const facts = useMemo(
		() =>
			factsOf(subject.trail, fields, {
				mentions: appearances.reduce(
					(total, group) => total + group.count,
					0,
				),
				first: appearances[0]?.title ?? null,
			}),
		[subject.trail, fields, appearances],
	);

	const links = useMemo(
		() =>
			corpus.kind === "ready"
				? linksOf(ties(fields, "relationships"), subjectsIn(documents))
				: [],
		[corpus.kind, documents, fields],
	);

	// A row on screen came out of the corpus, so its document is in hand.
	function known(id: string): ProjectDocument | undefined {
		return corpus.kind === "ready" ? corpus.known.get(id) : undefined;
	}

	function show(id: string) {
		const found = known(id);
		if (found !== undefined) {
			onOpen(found, null);
		}
	}

	function turn(id: string) {
		const found = known(id);
		if (found !== undefined) {
			onSubject(found);
		}
	}

	const remarks = text(fields, "remarks");
	// What the box holds while the writer is in it. For the 800 ms before the
	// file catches up, this is newer than anything read back from it.
	const [draft, setDraft] = useState<string | null>(null);
	const [trouble, setTrouble] = useState("");

	// Turning the page to another subject without remounting it.
	useEffect(() => {
		setDraft(null);
		setTrouble("");
	}, [subject.id]);

	// Once the file says what the box says, the box follows the file again,
	// so a change made anywhere else shows up here. Only on a match: a read
	// that lands while the writer is still typing is older than the box.
	useEffect(() => {
		if (draft !== null && draft === remarks) {
			setDraft(null);
		}
	}, [draft, remarks]);

	if (held.kind === "failed") {
		return (
			<section className="subject">
				<p className="subject__note subject__note--error">
					{held.message}
				</p>
			</section>
		);
	}

	// This page's copy of the file with the remarks replaced. `serialize` keeps
	// every other field spelled the way the file spells it.
	function rewritten(value: string): string {
		const { block, body } = split(held.kind === "ready" ? held.text : "");
		const next = new Map(fields);

		if (value === "") {
			next.delete("remarks");
		} else {
			next.set("remarks", value);
		}

		return `${serialize(next, block)}\n${body}`;
	}
	const place = subject.trail[0] === "Locations";
	const scenes = place ? "Scenes set here" : "Appears in";
	const connected = place ? "Who is here" : "Connected to";

	return (
		<section className="subject">
			<header className="subject__head">
				<div className="subject__who">
					<p className="subject__where">
						{subject.trail.join(" · ")}
					</p>
					<h2 className="subject__name">{subject.title}</h2>
					{facts.subtitle !== "" && (
						<p className="subject__lead">{facts.subtitle}</p>
					)}
				</div>
				<button
					type="button"
					className="subject__draft"
					onClick={onDraft}
				>
					Back to draft
				</button>
			</header>

			<div className="subject__boxes">
				{facts.boxes.map((box) => (
					<div className="subject__box" key={box.label}>
						<p className="subject__label">{box.label}</p>
						<p className="subject__value">{box.value}</p>
					</div>
				))}
			</div>

			<h3 className="subject__heading">Remarks</h3>
			<textarea
				className="subject__remarks"
				rows={7}
				value={draft ?? remarks}
				placeholder="Notes to yourself."
				disabled={held.kind !== "ready"}
				onChange={(event) => {
					const { value } = event.target;
					setDraft(value);
					setTrouble("");
					onRemarks(rewritten(value), setTrouble);
				}}
			/>
			<p className="subject__trouble">{trouble}</p>

			<h3 className="subject__heading">{scenes}</h3>
			{corpus.kind === "failed" ? (
				<p className="subject__note subject__note--error">
					{corpus.message}
				</p>
			) : corpus.kind !== "ready" ? (
				<p className="subject__note">Reading the project…</p>
			) : appearances.length === 0 ? (
				<p className="subject__note">
					Nothing names {subject.title} yet.
				</p>
			) : (
				<ul className="subject__appearances">
					{appearances.map((group) => (
						<li key={group.id}>
							<button
								type="button"
								className="subject__appearance"
								onClick={() => show(group.id)}
							>
								<span className="subject__title">
									{group.title}
								</span>
								<span className="subject__folder">
									{group.trail.join(" / ")}
								</span>
								<span className="subject__count">
									{group.count}
								</span>
							</button>
						</li>
					))}
				</ul>
			)}

			<h3 className="subject__heading">{connected}</h3>
			{links.length === 0 ? (
				<p className="subject__note">
					Nothing written yet. Relationships are added in the Info
					panel of the draft.
				</p>
			) : (
				<div className="subject__links">
					{links.map((link) => (
						<button
							key={link.name}
							type="button"
							className="subject__link"
							disabled={link.id === null}
							title={
								link.id === null
									? "No page in this project answers to that name"
									: undefined
							}
							onClick={() => {
								if (link.id !== null) {
									turn(link.id);
								}
							}}
						>
							{link.name}
							<span className="subject__tie">
								{link.note === "" ? "Not said yet" : link.note}
							</span>
						</button>
					))}
				</div>
			)}
		</section>
	);
}
