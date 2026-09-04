// The sessions the history file already holds, latest first, either across the
// project or filtered to one document. It is the only place a single session
// can be seen: every other number in the Stats tab adds them together, which is
// what makes a day's total and hides a morning's work.

import { useState } from "react";
import { moment } from "./dates";
import { lasted, newest, sessionKind, visits } from "./stats";
import type { PastSession } from "./types";

/** How many rows before the list caps itself, as search does. */
const CAP = 20;

type Row = {
	session: PastSession;
	written: number;
	removed: number;
	/** Left out for one document, which cannot own a share of the project's. */
	net?: number;
};

function Entry({ session, written, removed, net }: Row) {
	const numbers = [
		`written ${written.toLocaleString()}`,
		`removed ${removed.toLocaleString()}`,
		...(net === undefined ? [] : [`net ${net.toLocaleString()}`]),
	].join(" · ");

	return (
		<li className="history__session">
			<p className="history__head">
				<span>{moment(session.start)}</span>
				<span className="history__lasted">{lasted(session)}</span>
			</p>
			<p className="history__kind">{sessionKind(session)}</p>
			<p className="history__numbers">{numbers}</p>
		</li>
	);
}

export default function History({
	sessions,
	document,
}: {
	sessions: PastSession[];
	/** The document to keep to. Every session is listed without one. */
	document?: string;
}) {
	const [all, setAll] = useState(false);
	const rows: Row[] =
		document === undefined
			? newest(sessions).map((session) => ({
					session,
					written: session.written,
					removed: session.removed,
					net: session.net,
				}))
			: visits(sessions, document).map((visit) => ({
					session: visit.session,
					written: visit.written,
					removed: visit.removed,
				}));
	const capped = !all && rows.length > CAP;

	if (rows.length === 0) {
		return (
			<p className="history__none">
				{document === undefined
					? "No sessions yet."
					: "Nothing has been written here yet."}
			</p>
		);
	}

	return (
		<ul className="history">
			{(capped ? rows.slice(0, CAP) : rows).map((row) => (
				<Entry key={row.session.id} {...row} />
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
