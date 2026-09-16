// The sessions the history file already holds, grouped by day and latest first,
// either across the project or filtered to one document. It is the only place a
// single session can be seen: every other number in the Stats tab adds them
// together, which is what makes a day's total and hides a morning's work.

import { useState } from "react";
import { timeOfDay, when } from "./dates";
import {
	byDay,
	entries,
	firstOf,
	lasted,
	sessionKind,
	visits,
} from "./lib/stats";
import type { Entry } from "./lib/stats";
import type { PastSession } from "./types";

/** How many sessions before the list caps itself, as search does. */
const CAP = 20;

/** The numbers on one line, in the order they are counted. */
function numbers(of: { written: number; removed: number; net?: number }) {
	return [
		`written ${of.written.toLocaleString()}`,
		`removed ${of.removed.toLocaleString()}`,
		...(of.net === undefined ? [] : [`net ${of.net.toLocaleString()}`]),
	].join(" · ");
}

function Row({ session, written, removed, net }: Entry) {
	return (
		<li className="history__session">
			<p className="history__head">
				<span>{timeOfDay(session.start)}</span>
				<span className="history__lasted">{lasted(session)}</span>
			</p>
			<p className="history__kind">{sessionKind(session)}</p>
			<p className="history__numbers">
				{numbers({ written, removed, net })}
			</p>
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
	const list =
		document === undefined ? entries(sessions) : visits(sessions, document);
	const capped = !all && list.length > CAP;
	// Grouped before it is capped, so a day cut short by the cap still reports
	// what the whole day came to.
	const days = byDay(list);

	if (list.length === 0) {
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
			{(capped ? firstOf(days, CAP) : days).map((day) => (
				<li key={day.day} className="history__day">
					<p className="history__date">{when(day.at)}</p>
					<p className="history__numbers history__total">
						{numbers(day)}
					</p>
					<ul className="history__sessions">
						{day.entries.map((entry) => (
							<Row key={entry.session.id} {...entry} />
						))}
					</ul>
				</li>
			))}

			{capped && (
				<li>
					<button
						type="button"
						className="history__more"
						onClick={() => setAll(true)}
					>
						Show all {list.length}
					</button>
				</li>
			)}
		</ul>
	);
}
