// The sessions the history file already holds, latest first. It is the only
// place a single session can be seen: every other number in the Stats tab adds
// them together, which is what makes a day's total and hides a morning's work.

import { useState } from "react";
import { moment } from "./dates";
import { lasted, newest, sessionKind } from "./stats";
import type { PastSession } from "./types";

/** How many rows before the list caps itself, as search does. */
const CAP = 20;

export default function History({ sessions }: { sessions: PastSession[] }) {
	const [all, setAll] = useState(false);
	const rows = newest(sessions);
	const capped = !all && rows.length > CAP;

	if (rows.length === 0) {
		return <p className="history__none">No sessions yet.</p>;
	}

	return (
		<ul className="history">
			{(capped ? rows.slice(0, CAP) : rows).map((session) => (
				<li key={session.id} className="history__session">
					<p className="history__head">
						<span>{moment(session.start)}</span>
						<span className="history__lasted">
							{lasted(session)}
						</span>
					</p>
					<p className="history__kind">{sessionKind(session)}</p>
					<p className="history__numbers">
						{`written ${session.written.toLocaleString()} · removed ${session.removed.toLocaleString()} · net ${session.net.toLocaleString()}`}
					</p>
				</li>
			))}

			{capped && (
				<li>
					<button
						type="button"
						className="history__more"
						onClick={() => setAll(true)}
					>
						Show all {rows.length}
					</button>
				</li>
			)}
		</ul>
	);
}
