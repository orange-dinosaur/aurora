// The writing sessions of an open project: the model's state, the clock that
// closes them, and the history file they are written to.

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import {
	changed as asChange,
	recorded,
	start,
	stop,
	tick,
	wrote,
	IDLE,
	type Limit,
	type Running,
	type Sessions,
	type Step,
} from "./sessions";

export function useSessions(root: string, gap: number) {
	// Both layers of the session. A ref rather than state: the panel that shows
	// them is drawn from the same keystroke that moves them, since holding the
	// text is what renders this view anyway.
	const sessions = useRef<Sessions>(IDLE);

	// The deliberate layer again, as state this time. The titlebar draws it and
	// is nowhere near the keystroke that moves it, so it needs telling; the ref
	// above stays the one the model is handed.
	const [deliberate, setDeliberate] = useState<Running | null>(null);

	// Bumped every time a closed session reaches the history file, so the
	// bar and the Stats tab read it again.
	const [logged, setLogged] = useState(0);
	// Closed sessions the history file would not take yet.
	const unrecorded = useRef<ReturnType<typeof recorded>[]>([]);
	const [unrecordedCount, setUnrecordedCount] = useState(0);

	// What the project holds, which is where a session's net comes from. The
	// sidebar says where it stands every time it reads the manifest, and the
	// editor's changes carry it forward in between, so knowing this costs no
	// reading of its own.
	const words = useRef(0);

	// The sidebar counts the files, so what each document has gained since
	// its last save is added on.
	const ahead = useRef(new Map<string, number>());
	// Stable on purpose: the sidebar reloads its whole tree whenever the
	// handlers it was given change.
	const measured = useCallback((total: number) => {
		let unsaved = 0;
		ahead.current.forEach((delta) => {
			unsaved += delta;
		});
		words.current = total + unsaved;
	}, []);

	// The idle gap as it stands now, for handlers registered once.
	const idle = useRef(gap);
	useEffect(() => {
		idle.current = gap;
	});

	// The layers the model left behind are what runs from here, and whatever it
	// closed goes to the history file. Nothing reports a failure: it is written
	// behind the writer's back and there is nowhere to say so.
	// A session that could not be written is held and tried again with the
	// next, and Stats says so meanwhile.
	async function keep(step: Step) {
		sessions.current = step.sessions;
		setDeliberate(step.sessions.deliberate);
		const worth = step.closed.filter(
			(session) =>
				session.layer === "deliberate" ||
				session.written > 0 ||
				session.removed > 0,
		);
		const due = [...unrecorded.current, ...worth.map(recorded)];
		unrecorded.current = [];
		if (due.length === 0) {
			return;
		}

		const results = await Promise.allSettled(
			due.map((session) => invoke("append_session", { root, session })),
		);
		const failed = due.filter((_, at) => results[at].status === "rejected");
		unrecorded.current.push(...failed);
		setUnrecordedCount(unrecorded.current.length);
		setLogged((times) => times + 1);
	}

	// What a change added or removed, handed to the model, which decides which
	// sessions it opens, feeds and closes.
	function record(id: string, delta: number) {
		void keep(
			wrote(
				sessions.current,
				asChange(id, Date.now(), delta),
				words.current,
				gap,
			),
		);
		words.current += delta;
		ahead.current.set(id, (ahead.current.get(id) ?? 0) + delta);
	}

	// Opening a session by hand. A limit makes it a sprint, which closes itself
	// once the limit is met; without one it runs until the writer says stop.
	function startSession(limit?: Limit) {
		void keep(
			start(
				sessions.current,
				Date.now(),
				words.current,
				limit,
				idle.current,
			),
		);
	}

	function stopSession() {
		void keep(
			stop(sessions.current, Date.now(), words.current, idle.current),
		);
	}

	// What the clock alone has closed. The titlebar asks every second while a
	// session is running, so a sprint on minutes ends at its deadline rather
	// than at the next beat of the minute timer below.
	function tickSession() {
		void keep(tick(sessions.current, Date.now(), words.current, gap));
	}

	// Nobody else notices that a session has gone quiet, since going quiet is
	// the writer doing nothing. A minute is fine enough for a gap measured in
	// them, and it costs nothing when there is no session to close. Remade
	// when the gap changes, so the beat is never closing sessions by a length
	// the writer has moved on from.
	useEffect(() => {
		const beat = window.setInterval(
			() =>
				void keep(
					tick(sessions.current, Date.now(), words.current, gap),
				),
			60 * 1000,
		);
		return () => window.clearInterval(beat);
	}, [root, gap]);

	return {
		sessions,
		words,
		ahead,
		idle,
		deliberate,
		logged,
		unrecordedCount,
		measured,
		keep,
		record,
		startSession,
		stopSession,
		tickSession,
	};
}
