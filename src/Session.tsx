// The deliberate layer, as the writer touches it: Session in the bar, and the
// menu it opens to start one, give one a limit, or end one.

import { useEffect, useRef, useState } from "react";
import Icon from "./Icon";
import Menu, { MenuItem } from "./Menu";
import { sessionReading } from "./lib/stats";
import type { Limit, Running } from "./sessions";
import type { SprintUnit } from "./types";

/** What a sprint can be measured in, in the order the writer meets them. The
 * settings dialog offers the same pair from here, so the two cannot fall into
 * different orders. */
export const UNITS: SprintUnit[] = ["words", "minutes"];

/** What an empty sprint field stands for, and what it shows in grey. */
const USUAL_SPRINT = 500;

type Props = {
	/** The deliberate session running now, or null when there is none. */
	session: Running | null;
	/** Whether the clock is drawn. The menu and the beat below run either way. */
	shown: boolean;
	/** What the sprint field starts on, from the writer's preferences. */
	defaultAmount: number | null;
	defaultUnit: SprintUnit;
	/** Opens a session. A limit makes it a sprint. */
	onStart: (limit?: Limit) => void;
	onStop: () => void;
	/** Asks the model what the clock alone has closed, once a second. */
	onTick: () => void;
	/** The shortcut written out, for the menu. */
	shortcut: string;
};

/**
 * The session in the bar. It lives here because the rail and the binder can be
 * hidden and this row cannot, and a writer who opened a session needs to see
 * that it is still running without going looking for it.
 */
export default function Session({
	session,
	shown,
	defaultAmount,
	defaultUnit,
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
	// only while a session does. It runs with the clock turned off too.
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

	const clock =
		session !== null && shown ? sessionReading(session, now) : null;

	return (
		<Menu
			label={clock === null ? "Session" : `Session, ${clock}`}
			wrapper="titlebar__menu titlebar__menu--session"
			className="titlebar__quiet"
			trigger={
				<>
					<Icon name="play" />
					{clock === null ? (
						<span className="titlebar__label">Session</span>
					) : (
						<span className="titlebar__clock">{clock}</span>
					)}
				</>
			}
		>
			{(close) =>
				session === null ? (
					<>
						<MenuItem
							onSelect={() => {
								close();
								onStart();
							}}
						>
							Start session
							<span className="menu__chord">{shortcut}</span>
						</MenuItem>
						<Sprint
							defaultAmount={defaultAmount}
							defaultUnit={defaultUnit}
							onStart={(limit) => {
								close();
								onStart(limit);
							}}
						/>
					</>
				) : (
					<>
						<p className="menu__note">
							{session.limit === undefined ? "Session" : "Sprint"}
							: {sessionReading(session, now)}
						</p>
						<MenuItem
							onSelect={() => {
								close();
								onStop();
							}}
						>
							Stop session
							<span className="menu__chord">{shortcut}</span>
						</MenuItem>
					</>
				)
			}
		</Menu>
	);
}

/**
 * Starting a sprint, which is a session with something to reach. The menu
 * builds this each time it opens, so a default set in the settings dialog is
 * here the next time and the form starts folded away.
 */
function Sprint({
	defaultAmount,
	defaultUnit,
	onStart,
}: {
	/** What the field starts on. */
	defaultAmount: number | null;
	defaultUnit: SprintUnit;
	onStart: (limit: Limit) => void;
}) {
	const [asked, setAsked] = useState(false);
	const [amount, setAmount] = useState(
		defaultAmount === null ? "" : String(defaultAmount),
	);
	const [unit, setUnit] = useState<SprintUnit>(defaultUnit);
	const usual = (defaultAmount ?? USUAL_SPRINT).toLocaleString();
	const limit = amount === "" ? USUAL_SPRINT : Number(amount);

	// The form takes the row's place rather than opening under it, so the menu
	// stays the size it was.
	if (!asked) {
		return (
			<MenuItem onSelect={() => setAsked(true)}>
				Start a sprint
				<span className="menu__hint">
					{defaultUnit === "words"
						? `${usual} words, or a time limit`
						: `${usual} minutes, or a word count`}
				</span>
			</MenuItem>
		);
	}

	return (
		<form
			className="sprint"
			onSubmit={(event) => {
				event.preventDefault();
				onStart({ unit, amount: limit });
			}}
		>
			<input
				className="sprint__amount"
				value={amount}
				inputMode="numeric"
				aria-label="Sprint limit"
				placeholder={String(USUAL_SPRINT)}
				autoFocus
				onFocus={(event) => event.target.select()}
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
			<button type="submit" className="sprint__go" disabled={limit === 0}>
				Start
			</button>
		</form>
	);
}
