import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Target from "./Target";
import { failure } from "./errors";
import { running, today } from "./stats";
import type { Sessions } from "./sessions";
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
	root: string;
	/** Bumped when a session has been written to the history file. */
	logged: number;
	/** Bumped whenever the project changes on disk. */
	changed: number;
	/** The target the writer typed over the number, for whichever is open. */
	onTarget: (target: number | null) => void;
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

export default function Stats({
	page,
	text,
	folder,
	sessions,
	root,
	logged,
	changed,
	onTarget,
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

	const now = running(sessions, page.id);
	const day = today(history, sessions, page.id, Date.now());

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

			<h3 className="stats__heading">Today</h3>
			{trouble === null ? (
				<>
					<Layer name="Automatic" total={day.automatic} />
					<Layer name="Session" total={day.deliberate} />
				</>
			) : (
				<p className="stats__trouble">{trouble}</p>
			)}
		</div>
	);
}
