import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { reading } from "./Target";
import { failure } from "./errors";
import { running, today } from "./stats";
import type { Sessions } from "./sessions";
import type { PastSession, ProjectDocument } from "./types";
import { prose, words } from "./words";

type Props = {
	/** The open document, or null when a folder's overview is showing. */
	page: ProjectDocument | null;
	/** Its text as its tab holds it, so this counts what is on screen. */
	text: string;
	/** The two layers as they stand, for the numbers that are not on disk yet. */
	sessions: Sessions;
	root: string;
	/** Bumped when a session has been written to the history file. */
	logged: number;
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

export default function Stats({ page, text, sessions, root, logged }: Props) {
	const [history, setHistory] = useState<PastSession[]>([]);
	const [trouble, setTrouble] = useState<string | null>(null);
	const counted = useMemo(() => words(prose(text)), [text]);

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

	if (page === null) {
		return <p className="right-sidebar__empty">Nothing here yet.</p>;
	}

	const now = running(sessions, page.id);
	const day = today(history, sessions, page.id, Date.now());

	return (
		<div className="stats">
			<p className="stats__words">{reading(counted, page.target)}</p>

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
