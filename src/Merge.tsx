// The merge screen for a document: its merged text, with each spot split into
// this version's words and the other's, settled one spot at a time. The
// choices belong to the project until Done writes them, so going back to
// writing loses none.

import { invoke } from "@tauri-apps/api/core";
import { diffWordsWithSpace } from "diff";
import { useEffect, useRef, useState } from "react";
import { failure } from "./errors";
import { split } from "./frontmatter";
import Icon from "./Icon";
import {
	joined,
	left,
	leftLabel,
	nextOpen,
	paragraphs,
	settled,
	spotCount,
	spotsLeft,
	type Choice,
	type Choices,
} from "./lib/merge";
import { settleLines, type MergeUnderway } from "./lib/versions";
import type { Piece } from "./types";

type Props = {
	root: string;
	merge: MergeUnderway;
	/** The id of the document being settled. */
	open: string;
	onOpen: (id: string) => void;
	/** What the writer has chosen so far, by document. */
	choices: Record<string, Choices>;
	onChoices: (id: string, choices: Choices) => void;
	onBack: () => void;
	/** Writes everything down, runs the task, and catches the tabs up. */
	onSettle: (task: () => Promise<unknown>) => Promise<void>;
	/** Reads the merge again once a document is settled. */
	onMergeChanged: () => Promise<void>;
};

/** The folder a document sits in, by name. */
function where(path: string | null): string {
	return path?.split("/").slice(-2, -1)[0] ?? "";
}

export default function Merge({
	root,
	merge,
	open,
	onOpen,
	choices,
	onChoices,
	onBack,
	onSettle,
	onMergeChanged,
}: Props) {
	const { merging, into } = merge;
	const theirs = merging.name ?? "the other version";
	const [pieces, setPieces] = useState<Record<string, Piece[]>>({});
	const [current, setCurrent] = useState(0);
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const spots = useRef<(HTMLDivElement | null)[]>([]);

	useEffect(() => {
		let live = true;
		const ids = merging.conflicts.flatMap((conflict) =>
			conflict.kind === "text" && conflict.id !== null
				? [conflict.id]
				: [],
		);
		Promise.all(
			ids.map(async (id) => {
				const found = await invoke<Piece[] | null>("merge_spots", {
					root,
					id,
				});
				return [id, found ?? []] as const;
			}),
		)
			.then((found) => {
				if (live) {
					setPieces(Object.fromEntries(found));
					setError(null);
				}
			})
			.catch((reason: unknown) => {
				if (live) {
					setError(failure(reason).message);
				}
			});
		return () => {
			live = false;
		};
	}, [root, merging]);

	const counts = Object.fromEntries(
		Object.entries(pieces).map(([id, found]) => [id, spotCount(found)]),
	);
	const lines = settleLines(merging, counts);
	const documents = lines.filter((line) => line.kind === "document");
	const binder = lines.find((line) => line.kind === "binder");
	const structural = merging.conflicts.filter(
		(conflict) => conflict.kind !== "text",
	).length;
	const line = documents.find((document) => document.id === open);
	const shown = pieces[open] ?? [];
	const chosen = choices[open] ?? [];
	const total = spotCount(shown);
	const remaining = left(shown, chosen);

	// Each document opens on its first spot still to choose.
	useEffect(() => {
		setCurrent(Math.max(0, nextOpen(choices[open] ?? [], total, -1)));
	}, [open, total]);

	useEffect(() => {
		spots.current[current]?.scrollIntoView({ block: "nearest" });
	}, [current, open]);

	// Sized to every spot, so a choice can land on any of them.
	function chose(update: (next: Choices) => void): Choices {
		const next = Array.from(
			{ length: total },
			(_, at) => chosen[at] ?? null,
		);
		update(next);
		onChoices(open, next);
		return next;
	}

	function choose(at: number, choice: Choice) {
		const next = chose((next) => {
			next[at] = choice;
		});
		const after = nextOpen(next, total, at);
		if (after !== -1) {
			setCurrent(after);
		}
	}

	function everywhere(choice: Choice) {
		chose((next) =>
			next.forEach((was, at) => {
				next[at] = was ?? choice;
			}),
		);
	}

	function change(at: number) {
		chose((next) => {
			next[at] = null;
		});
		setCurrent(at);
	}

	async function done() {
		const text = joined(shown, chosen);
		if (text === null) {
			return;
		}
		setBusy(true);
		try {
			await onSettle(() =>
				invoke("settle_document", { root, id: open, text }),
			);
			onChoices(open, []);
			await onMergeChanged();
			const next = documents.find(
				(document) => document.id !== null && document.id !== open,
			);
			if (next?.id) {
				onOpen(next.id);
			} else {
				onBack();
			}
		} catch (reason) {
			setError(failure(reason).message);
		} finally {
			setBusy(false);
		}
	}

	const took = (choice: Choice) =>
		choice === "ours"
			? "kept mine"
			: choice === "theirs"
				? `took ${theirs}'s`
				: "kept both";

	let seen = 0;
	const numbered = shown.map((piece) =>
		piece.kind === "spot" ? { piece, at: seen++ } : { piece, at: -1 },
	);

	return (
		<div className="compare">
			<div className="compare__bar">
				<button
					type="button"
					className="compare__back"
					onClick={onBack}
				>
					<Icon name="chevron-left" />
					Back to writing
				</button>
			</div>

			<div className="compare__body">
				<nav className="compare__history" aria-label="To settle">
					<h2 className="compare__title">
						Merging {merging.name ?? "another version"}
					</h2>
					<p className="compare__trail merge__trail">
						into {into} · {lines.length.toLocaleString()} to settle
					</p>
					{documents.map((document) => {
						const found =
							document.id === null
								? undefined
								: pieces[document.id];
						const count =
							found === undefined ? 0 : spotCount(found);
						const still =
							found === undefined || document.id === null
								? 0
								: left(found, choices[document.id] ?? []);
						return (
							<button
								key={document.key}
								type="button"
								className="merge__pick"
								aria-current={document.id === open}
								disabled={document.id === null}
								onClick={() => {
									if (document.id !== null) {
										onOpen(document.id);
									}
								}}
							>
								<span className="merge__pick-text">
									<span className="merge__pick-name">
										{document.name}
									</span>
									<span className="merge__pick-where">
										{where(document.path)}
									</span>
								</span>
								<span
									className={
										found !== undefined && still === 0
											? "merge__pick-left merge__pick-left--done"
											: "merge__pick-left"
									}
								>
									{found === undefined
										? ""
										: leftLabel(still, count)}
								</span>
							</button>
						);
					})}
					{binder !== undefined && (
						<>
							<div className="merge__rule" />
							{/* Settled on a screen of its own, still to come. */}
							<button
								type="button"
								className="merge__pick"
								disabled
							>
								<span className="merge__pick-text">
									<span className="merge__pick-name">
										{binder.name}
									</span>
									<span className="merge__pick-where">
										Names, places, order, fields, cover
									</span>
								</span>
								<span className="merge__pick-left">
									{leftLabel(structural, structural)}
								</span>
							</button>
						</>
					)}
				</nav>

				<div className="compare__main">
					{line !== undefined && (
						<>
							<div className="merge__head">
								<span className="merge__name">{line.name}</span>
								<span className="merge__detail">
									{[
										where(line.path),
										spotsLeft(remaining, total),
									]
										.filter((part) => part !== "")
										.join(" · ")}
								</span>
								<span className="merge__all">
									<button
										type="button"
										className="compare__step"
										disabled={remaining === 0}
										onClick={() => everywhere("ours")}
									>
										Keep mine everywhere
									</button>
									<button
										type="button"
										className="compare__step"
										disabled={remaining === 0}
										onClick={() => everywhere("theirs")}
									>
										Take {theirs}'s everywhere
									</button>
								</span>
							</div>

							<div className="merge__scroll">
								<div className="merge__prose">
									{numbered.map(({ piece, at }, index) => {
										if (piece.kind === "clean") {
											// The file's own fields are not prose.
											const text =
												index === 0
													? split(piece.text).body
													: piece.text;
											return paragraphs(text).map(
												(paragraph, n) => (
													<p key={`${index}-${n}`}>
														{paragraph}
													</p>
												),
											);
										}
										const choice = chosen[at] ?? null;
										return choice === null ? (
											<Spot
												key={index}
												spot={piece}
												at={at}
												total={total}
												current={at === current}
												into={into}
												theirs={theirs}
												onChoose={(choice) =>
													choose(at, choice)
												}
												place={(element) => {
													spots.current[at] = element;
												}}
											/>
										) : (
											<Settled
												key={index}
												text={settled(piece, choice)}
												note={`Spot ${at + 1} · ${took(choice)} · `}
												onChange={() => change(at)}
											/>
										);
									})}
								</div>
							</div>
						</>
					)}

					<div className="compare__foot">
						{line !== undefined && total > 0 && (
							<>
								<span className="compare__count">
									Spot {current + 1} of {total}
								</span>
								<button
									type="button"
									className="compare__step"
									disabled={current === 0}
									onClick={() => setCurrent(current - 1)}
								>
									‹ Previous
								</button>
								<button
									type="button"
									className="compare__step"
									disabled={current >= total - 1}
									onClick={() => setCurrent(current + 1)}
								>
									Next ›
								</button>
							</>
						)}
						<button
							type="button"
							className="compare__put"
							disabled={
								busy ||
								line === undefined ||
								total === 0 ||
								remaining > 0
							}
							onClick={() => void done()}
						>
							Done with {line?.name ?? "this document"}
						</button>
						{/* A message takes the note's line, so it moves nothing. */}
						<p className="compare__note" role="alert">
							{error ?? ""}
						</p>
					</div>
				</div>
			</div>
		</div>
	);
}

// A spot still to choose: each version's words side by side, and the three
// ways to settle it.
function Spot({
	spot,
	at,
	total,
	current,
	into,
	theirs,
	onChoose,
	place,
}: {
	spot: { ours: string; theirs: string };
	at: number;
	total: number;
	current: boolean;
	into: string;
	theirs: string;
	onChoose: (choice: Choice) => void;
	/** Hands the spot's element over, so stepping can scroll to it. */
	place: (element: HTMLDivElement | null) => void;
}) {
	return (
		<div
			ref={place}
			className={
				current ? "merge-spot merge-spot--current" : "merge-spot"
			}
		>
			<div className="merge-spot__sides">
				<Side
					label={`${into} · yours`}
					text={spot.ours}
					other={spot.theirs}
					mine
				/>
				<Side
					label={theirs}
					text={spot.theirs}
					other={spot.ours}
					mine={false}
				/>
			</div>
			<div className="merge-spot__choose">
				<button
					type="button"
					className="merge-spot__choice"
					onClick={() => onChoose("ours")}
				>
					Keep mine
				</button>
				<button
					type="button"
					className="merge-spot__choice"
					onClick={() => onChoose("theirs")}
				>
					Take {theirs}'s
				</button>
				<button
					type="button"
					className="merge-spot__choice"
					onClick={() => onChoose("both")}
				>
					Keep both
				</button>
				<span className="merge-spot__n">
					Spot {at + 1} of {total}
				</span>
			</div>
		</div>
	);
}

// A chosen spot, reading as text again, with a way to reopen it.
function Settled({
	text,
	note,
	onChange,
}: {
	text: string;
	note: string;
	onChange: () => void;
}) {
	return (
		<div className="merge__settled">
			{paragraphs(text).map((paragraph, n) => (
				<p key={n}>{paragraph}</p>
			))}
			<p className="merge__settled-note">
				{note}
				<button
					type="button"
					className="compare__step"
					onClick={onChange}
				>
					Change
				</button>
			</p>
		</div>
	);
}

// One version's words in a spot, with what the other version does not have
// washed in.
function Side({
	label,
	text,
	other,
	mine,
}: {
	label: string;
	text: string;
	other: string;
	mine: boolean;
}) {
	const parts = diffWordsWithSpace(
		mine ? text.trimEnd() : other.trimEnd(),
		mine ? other.trimEnd() : text.trimEnd(),
	);
	return (
		<div
			className={
				mine
					? "merge-spot__side merge-spot__side--mine"
					: "merge-spot__side merge-spot__side--theirs"
			}
		>
			<p className="merge-spot__label">{label}</p>
			<p className="merge-spot__text">
				{text.trim() === "" ? (
					<span className="merge-spot__empty">Nothing here.</span>
				) : (
					parts
						.filter((part) => (mine ? !part.added : !part.removed))
						.map((part, at) =>
							part.added || part.removed ? (
								<span key={at} className="merge-spot__wash">
									{part.value}
								</span>
							) : (
								part.value
							),
						)
				)}
			</p>
		</div>
	);
}
