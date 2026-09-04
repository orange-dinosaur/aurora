import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import History from "./History";
import Target from "./Target";
import { Sprint } from "./Session";
import { failure } from "./errors";
import { dayTally, running, today, unexplained } from "./stats";
import type { Tally } from "./stats";
import type { Limit, Sessions } from "./sessions";
import type { FolderRef } from "./tree";
import type { FolderProgress, PastSession, ProjectDocument } from "./types";
import { prose, words } from "./words";

type Props = {
	/** The open document, or null when a folder's overview is showing. */
	page: ProjectDocument | null;
	/** Its text as its tab holds it, so this counts what is on screen. */
	text: string;
	/** The folder whose overview is showing, when it is one of those. */
	folder: FolderRef | null;
	/** The two layers as they stand, for the numbers that are not on disk yet. */
	sessions: Sessions;
	/** What the whole project holds, which is where a running session's net ends. */
	words: number;
	root: string;
	/** Bumped when a session has been written to the history file. */
	logged: number;
	/** Bumped whenever the project changes on disk. */
	changed: number;
	/** The target the writer typed over the number, for whichever is open. */
	onTarget: (target: number | null) => void;
	/** Opens a deliberate session with something to reach. */
	onStartSprint: (limit: Limit) => void;
	onStopSession: () => void;
};

/**
 * One number, which always says which layer it came from. A session the writer
 * opened and one detected underneath them are different claims, so they are
 * never added together or left unlabelled.
 */
function Layer({ name, total }: { name: string; total: number | null }) {
	return (
		<p className="stats__line">
			<span className="stats__layer">{name}</span>
			<span className="stats__number">
				{total === null ? "none" : `${total.toLocaleString()} words`}
			</span>
		</p>
	);
}

/** One of the three, so a row of them lines up whatever is in it. */
function Count({ name, total }: { name: string; total: number }) {
	return (
		<p className="stats__line">
			<span className="stats__layer">{name}</span>
			<span className="stats__number">{total.toLocaleString()}</span>
		</p>
	);
}

/**
 * One layer's three project numbers, and the line that reconciles them when
 * they do not add up. Nothing here picks one to believe: net sees a document
 * deleted whole and the other two never can, so both readings are true and the
 * panel says which is which.
 */
function Whole({ name, tally }: { name: string; tally: Tally }) {
	const gap = unexplained(tally);

	return (
		<>
			<h4 className="stats__layers">{name}</h4>
			<Count name="Written" total={tally.written} />
			<Count name="Removed" total={tally.removed} />
			<Count name="Net" total={tally.net} />
			{gap !== 0 && (
				<p className="stats__note">
					{gap < 0
						? `Net is ${(-gap).toLocaleString()} lower: a document deleted whole shows here and in neither of the others.`
						: `Net is ${gap.toLocaleString()} higher: words arrived from outside the editor.`}
				</p>
			)}
		</>
	);
}

export default function Stats({
	page,
	text,
	folder,
	sessions,
	words: held,
	root,
	logged,
	changed,
	onTarget,
	onStartSprint,
	onStopSession,
}: Props) {
	const [history, setHistory] = useState<PastSession[]>([]);
	const [progress, setProgress] = useState<FolderProgress | null>(null);
	const [trouble, setTrouble] = useState<string | null>(null);
	const counted = useMemo(() => words(prose(text)), [text]);
	const id = folder?.id ?? null;

	// Read when the tab opens and again whenever a session has been added to
	// the file. Nothing else changes it.
	useEffect(() => {
		let listening = true;
		invoke<PastSession[]>("read_history", { root })
			.then((sessions) => {
				if (listening) {
					setHistory(sessions);
					setTrouble(null);
				}
			})
			.catch((error) => {
				if (listening) {
					setHistory([]);
					setTrouble(failure(error).message);
				}
			});

		return () => {
			listening = false;
		};
	}, [root, logged]);

	// A folder's total comes from the files, so it is asked for again whenever
	// the project has changed on disk, a target of its own included.
	useEffect(() => {
		if (id === null) {
			setProgress(null);
			return;
		}

		let listening = true;
		invoke<FolderProgress>("folder_progress", { root, id })
			.then((read) => {
				if (listening) {
					setProgress(read);
				}
			})
			.catch(() => {
				if (listening) {
					setProgress(null);
				}
			});

		return () => {
			listening = false;
		};
	}, [root, id, changed]);

	// A folder is its total against its target and nothing else. The session
	// numbers are counted where the editor sees them, one document at a time.
	if (page === null) {
		return progress === null ? (
			<p className="right-sidebar__empty">Nothing here yet.</p>
		) : (
			<div className="stats">
				{/* A div, not a paragraph: the control becomes a form when
				    the writer types a target into it. */}
				<div className="stats__words">
					<Target
						words={progress.words}
						target={progress.target}
						onTarget={onTarget}
					/>
				</div>
			</div>
		);
	}

	const at = Date.now();
	const now = running(sessions, page.id);
	const day = today(history, sessions, page.id, at);
	const project = dayTally(history, sessions, held, at);

	return (
		<div className="stats">
			<div className="stats__words">
				<Target
					words={counted}
					target={page.target}
					onTarget={onTarget}
				/>
			</div>

			<h3 className="stats__heading">Now</h3>
			<Layer name="Automatic" total={now.automatic} />
			<Layer
				name="Session"
				total={sessions.deliberate === null ? null : now.deliberate}
			/>

			<h3 className="stats__heading">Sprint</h3>
			<Sprint
				session={sessions.deliberate}
				onStart={onStartSprint}
				onStop={onStopSession}
			/>

			<h3 className="stats__heading">Today</h3>
			{trouble === null ? (
				<>
					<Layer name="Automatic" total={day.automatic} />
					<Layer name="Session" total={day.deliberate} />
				</>
			) : (
				<p className="stats__trouble">{trouble}</p>
			)}

			{/* The three numbers are project-wide because that is where net is
			    measured: a document deleted whole never passes the editor, so
			    there is no document to hang it on. */}
			{trouble === null && (
				<>
					<h3 className="stats__heading">The project, today</h3>
					<Whole name="Automatic" tally={project.automatic} />
					<Whole name="Session" tally={project.deliberate} />

					{/* Every number above this adds sessions together. This is
					    where one of them can be looked at on its own. */}
					<h3 className="stats__heading">Past sessions</h3>
					<History sessions={history} />
				</>
			)}
		</div>
	);
}
