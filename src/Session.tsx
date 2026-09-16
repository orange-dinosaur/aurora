// The deliberate layer, as the writer touches it: the readout in the titlebar
// that says a session is running and ends it, and the form in the Stats tab
// that gives one a limit.

import { useEffect, useRef, useState } from "react";
import Icon from "./Icon";
import { sessionReading } from "./lib/stats";
import type { Limit, Running } from "./sessions";
import type { SprintUnit } from "./types";

/** What a sprint can be measured in, in the order the writer meets them. The
 * settings dialog offers the same pair from here, so the two cannot fall into
 * different orders. */
export const UNITS: SprintUnit[] = ["words", "minutes"];

type Props = {
	/** The deliberate session running now, or null when there is none. */
	session: Running | null;
	/** Whether the readout is drawn. The beat below runs either way. */
	shown: boolean;
	onStart: () => void;
	onStop: () => void;
	/** Asks the model what the clock alone has closed, once a second. */
	onTick: () => void;
	/** The shortcut written out, for the tooltip. */
	shortcut: string;
};

/**
 * The session in the titlebar. It lives here because the toolbar and both
 * sidebars can be hidden and this row cannot, and a writer who opened a session
 * needs to see that it is still running without going looking for it.
 */
export default function Session({
	session,
	shown,
	onStart,
	onStop,
	onTick,
	shortcut,
}: Props) {
	const [now, setNow] = useState(Date.now());
	const running = session !== null;

	// The handler is rebuilt on every render of the view above, and the timer
	// below must not be torn down and remade each time to keep up with it.
	const beat = useRef(onTick);
	useEffect(() => {
		beat.current = onTick;
	});

	// A sprint on the clock has to close at its deadline rather than whenever
	// anyone next looked, and the readout counts in seconds either way. The
	// minute heartbeat elsewhere is too coarse for both, and this one exists
	// only while a session does.
	useEffect(() => {
		if (!running) {
			return;
		}

		setNow(Date.now());
		const timer = window.setInterval(() => {
			setNow(Date.now());
			beat.current();
		}, 1000);

		return () => window.clearInterval(timer);
	}, [running]);

	// The writer can turn the readout off, but not the beat above it: a sprint
	// counted in minutes has to close at its deadline whether or not anyone is
	// watching the clock.
	if (!shown) {
		return null;
	}

	// The readout is the control, the way the word count is: no box of its own,
	// and pressing it ends what it is describing.
	return session === null ? (
		<button
			type="button"
			className="titlebar__session titlebar__session--idle"
			aria-label="Start session"
			title={`Start a writing session (${shortcut})`}
			onClick={onStart}
		>
			<Icon name="timer" />
			<span className="titlebar__label">Start session</span>
		</button>
	) : (
		<button
			type="button"
			className="titlebar__session titlebar__session--running"
			title={`Stop this session (${shortcut})`}
			onClick={onStop}
		>
			{sessionReading(session, now)}
		</button>
	);
}

/**
 * Starting a sprint, which is a session with something to reach. It is in the
 * Stats tab rather than the titlebar because a limit has to be typed, and this
 * is where the numbers it will move are already shown.
 */
export function Sprint({
	session,
	defaultAmount,
	defaultUnit,
	onStart,
	onStop,
}: {
	session: Running | null;
	/** What the field starts on, and returns to after a sprint is started. */
	defaultAmount: number | null;
	defaultUnit: SprintUnit;
	onStart: (limit: Limit) => void;
	onStop: () => void;
}) {
	const start = defaultAmount === null ? "" : String(defaultAmount);
	const [amount, setAmount] = useState(start);
	const [unit, setUnit] = useState<SprintUnit>(defaultUnit);

	if (session !== null) {
		return (
			<p className="stats__line">
				<span className="stats__layer">
					{session.limit === undefined
						? "Running"
						: `${session.limit.amount.toLocaleString()} ${session.limit.unit}`}
				</span>
				<button type="button" className="sprint__stop" onClick={onStop}>
					Stop
				</button>
			</p>
		);
	}

	return (
		<form
			className="sprint"
			onSubmit={(event) => {
				event.preventDefault();
				onStart({ unit, amount: Number(amount) });
				setAmount(start);
			}}
		>
			<input
				className="sprint__amount"
				value={amount}
				inputMode="numeric"
				aria-label="Sprint limit"
				placeholder="500"
				// Digits only: a limit is a count either way, and refusing the
				// rest here saves explaining it afterwards.
				onChange={(event) =>
					setAmount(event.target.value.replace(/\D/g, ""))
				}
			/>
			{/* Two buttons rather than a select. WebKitGTK draws a native
			    dropdown in the desktop theme's colours whatever the page asks
			    for, and nothing else in Aurora is a native widget either. */}
			<span
				className="sprint__units"
				role="group"
				aria-label="What the sprint is counting"
			>
				{UNITS.map((it) => (
					<button
						key={it}
						type="button"
						className="sprint__unit"
						aria-pressed={unit === it}
						onClick={() => setUnit(it)}
					>
						{it}
					</button>
				))}
			</span>
			<button
				type="submit"
				className="sprint__go"
				disabled={amount === "" || Number(amount) === 0}
			>
				Start
			</button>
		</form>
	);
}
