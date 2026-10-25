// The merge screen: a document's merged text, with each spot split into this
// version's words and the other's, settled one spot at a time, or the binder
// and details as one card per conflict. The choices belong to the project
// until Done writes them, so going back to writing loses none.

import { invoke } from "@tauri-apps/api/core";
import { diffWordsWithSpace } from "diff";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { failure } from "./errors";
import { split } from "./frontmatter";
import Icon from "./Icon";
import {
	BINDER,
	cardKey,
	cards,
	cardsLeft,
	fieldText,
	fieldTitle,
	joined,
	left,
	leftLabel,
	named,
	nextOpen,
	paragraphs,
	settled,
	spotCount,
	spotsLeft,
	theirSides,
	type Choice,
	type Choices,
	type Side,
	type Sides,
} from "./lib/merge";
import { settleLines, type MergeUnderway } from "./lib/versions";
import type { Conflict, Piece, TreeNode } from "./types";

type Props = {
	root: string;
	merge: MergeUnderway;
	/** The id of the document being settled, or `BINDER`. */
	open: string;
	onOpen: (id: string) => void;
	/** What the writer has chosen so far, by document. */
	choices: Record<string, Choices>;
	onChoices: (id: string, choices: Choices) => void;
	/** What the writer has chosen so far on the binder's cards. */
	sides: Sides;
	onSides: (sides: Sides) => void;
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
	sides,
	onSides,
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
	const binderCards = cards(merging.conflicts);
	const cardsOpen = cardsLeft(binderCards, sides);
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
			finished();
		} catch (reason) {
			setError(failure(reason).message);
		} finally {
			setBusy(false);
		}
	}

	// Once something is settled, on to the next thing left, or back to
	// writing when nothing is.
	function finished() {
		const next = documents.find(
			(document) => document.id !== null && document.id !== open,
		);
		if (next?.id) {
			onOpen(next.id);
		} else if (binder !== undefined && open !== BINDER) {
			onOpen(BINDER);
		} else {
			onBack();
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
							<button
								type="button"
								className="merge__pick"
								aria-current={open === BINDER}
								onClick={() => onOpen(BINDER)}
							>
								<span className="merge__pick-text">
									<span className="merge__pick-name">
										{binder.name}
									</span>
									<span className="merge__pick-where">
										Names, places, order, fields, cover
									</span>
								</span>
								<span
									className={
										cardsOpen === 0
											? "merge__pick-left merge__pick-left--done"
											: "merge__pick-left"
									}
								>
									{leftLabel(cardsOpen, binderCards.length)}
								</span>
							</button>
						</>
					)}
				</nav>

				<div className="compare__main">
					{open === BINDER && (
						<Binder
							root={root}
							cards={binderCards}
							sides={sides}
							onSides={onSides}
							into={into}
							theirs={theirs}
							onSettle={onSettle}
							onMergeChanged={onMergeChanged}
							onFinished={finished}
						/>
					)}

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

					{open !== BINDER && (
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
					)}
				</div>
			</div>
		</div>
	);
}

// The binder and details: a card per conflict, each keeping one version's
// side. Done writes every card at once.
function Binder({
	root,
	cards,
	sides,
	onSides,
	into,
	theirs,
	onSettle,
	onMergeChanged,
	onFinished,
}: {
	root: string;
	cards: Conflict[];
	sides: Sides;
	onSides: (sides: Sides) => void;
	into: string;
	theirs: string;
	onSettle: (task: () => Promise<unknown>) => Promise<void>;
	onMergeChanged: () => Promise<void>;
	onFinished: () => void;
}) {
	const [names, setNames] = useState<ReturnType<typeof named>>(new Map());
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const remaining = cardsLeft(cards, sides);

	useEffect(() => {
		let live = true;
		invoke<TreeNode[]>("document_tree", { root })
			.then((tree) => {
				if (live) {
					setNames(named(tree));
				}
			})
			.catch(() => {
				// Each card falls back to the names the merge carries.
			});
		return () => {
			live = false;
		};
	}, [root]);

	function everywhere(side: Side) {
		onSides(
			Object.fromEntries(
				cards.map((card) => [
					cardKey(card),
					sides[cardKey(card)] ?? side,
				]),
			),
		);
	}

	async function done() {
		const chosen = theirSides(cards, sides);
		if (chosen === null) {
			return;
		}
		setBusy(true);
		try {
			await onSettle(() =>
				invoke("settle_binder", { root, theirs: chosen }),
			);
			onSides({});
			await onMergeChanged();
			onFinished();
		} catch (reason) {
			setError(failure(reason).message);
		} finally {
			setBusy(false);
		}
	}

	return (
		<>
			<div className="merge__head">
				<span className="merge__name">Binder and details</span>
				<span className="merge__detail">
					{leftLabel(remaining, cards.length)}
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
				<div className="merge-cards">
					{cards.map((card) => (
						<Card
							key={cardKey(card)}
							root={root}
							card={card}
							names={names}
							into={into}
							theirs={theirs}
							side={sides[cardKey(card)] ?? null}
							onSide={(side) =>
								onSides({ ...sides, [cardKey(card)]: side })
							}
						/>
					))}
				</div>
			</div>

			<div className="compare__foot">
				<span className="compare__count">
					{leftLabel(remaining, cards.length)}
				</span>
				<button
					type="button"
					className="compare__put"
					disabled={busy || cards.length === 0 || remaining > 0}
					onClick={() => void done()}
				>
					Done with binder and details
				</button>
				<p className="compare__note" role="alert">
					{error ?? ""}
				</p>
			</div>
		</>
	);
}

// One conflict outside the text, and the two sides it can keep. Names come
// from the tree as it stands, which is this version's, and fall back to what
// the conflict carries.
function Card({
	root,
	card,
	names,
	into,
	theirs,
	side,
	onSide,
}: {
	root: string;
	card: Conflict;
	names: ReturnType<typeof named>;
	into: string;
	theirs: string;
	side: Side | null;
	onSide: (side: Side) => void;
}) {
	const folder = (id: string | null) =>
		id === null
			? "The top level"
			: (names.get(id)?.name ?? `A folder only ${theirs} has`);
	const order = (ids: string[]) => (
		<span className="merge-card__order">
			{ids.map((id, at) => (
				<span key={id}>
					{at + 1}. {names.get(id)?.name ?? `Only in ${theirs}`}
				</span>
			))}
		</span>
	);

	let kind: string;
	let what: string;
	let where = "";
	let mine: ReactNode;
	let other: ReactNode;
	switch (card.kind) {
		case "text":
			return null;
		case "renamed":
			kind = "Renamed on both";
			what = names.get(card.id)?.name ?? card.ours;
			where = names.get(card.id)?.folder ?? "";
			mine = <Value text={card.ours} />;
			other = <Value text={card.theirs} />;
			break;
		case "moved":
			kind = "Moved on both";
			what = names.get(card.id)?.name ?? "A folder";
			mine = <Value text={folder(card.ours)} />;
			other = <Value text={folder(card.theirs)} />;
			break;
		case "deleted": {
			kind = "Deleted and edited";
			what = names.get(card.id)?.name ?? card.name;
			where = names.get(card.id)?.folder ?? "";
			const many =
				card.documents === 1
					? ""
					: `${card.documents.toLocaleString()} documents`;
			if (card.here) {
				mine = <Value text="Delete it" more="You deleted it." />;
				other = (
					<Value
						text={`Keep it, with ${theirs}'s edits`}
						more={many === "" ? "" : `That brings back ${many}.`}
					/>
				);
			} else {
				mine = <Value text="Keep it, with your edits" />;
				other = (
					<Value
						text="Delete it"
						more={`${theirs} deleted it.${many === "" ? "" : ` It holds ${many}.`}`}
					/>
				);
			}
			break;
		}
		case "reordered":
			kind = "Reordered on both";
			what = card.id === null ? "The top level" : folder(card.id);
			mine = order(card.ours);
			other = order(card.theirs);
			break;
		case "field":
			kind = "Changed on both";
			what = fieldTitle(
				card.field,
				card.id === null
					? null
					: (names.get(card.id)?.name ?? "a folder"),
			);
			mine = <Value text={fieldText(card.ours)} prose />;
			other = <Value text={fieldText(card.theirs)} prose />;
			break;
		case "cover":
			kind = "Changed on both";
			what = "Cover";
			mine = <Thumb root={root} theirs={false} file={card.ours} />;
			other = <Thumb root={root} theirs file={card.theirs} />;
			break;
	}

	return (
		<section className="merge-card">
			<header className="merge-card__head">
				<span className="merge-card__kind">{kind}</span>
				<span className="merge-card__what">{what}</span>
				{where !== "" && (
					<span className="merge-card__where">{where}</span>
				)}
				<span
					className={
						side === null
							? "merge-card__state"
							: "merge-card__state merge-card__state--chosen"
					}
				>
					{side === null ? "To choose" : "Chosen"}
				</span>
			</header>
			<div className="merge-card__options">
				<button
					type="button"
					className="merge-card__option"
					aria-pressed={side === "ours"}
					onClick={() => onSide("ours")}
				>
					<span className="merge-card__side">{into}</span>
					{mine}
				</button>
				<button
					type="button"
					className="merge-card__option"
					aria-pressed={side === "theirs"}
					onClick={() => onSide("theirs")}
				>
					<span className="merge-card__side">{theirs}</span>
					{other}
				</button>
			</div>
		</section>
	);
}

// What one side of a card keeps, with a line under it when there is more to say.
function Value({
	text,
	more = "",
	prose = false,
}: {
	text: string;
	more?: string;
	prose?: boolean;
}) {
	return (
		<>
			{text === "" ? (
				<span className="merge-spot__empty">Nothing here.</span>
			) : (
				<span
					className={
						prose ? "merge-card__prose" : "merge-card__value"
					}
				>
					{text}
				</span>
			)}
			{more !== "" && <span className="merge-card__more">{more}</span>}
		</>
	);
}

// One side's cover, small, with its file's name.
function Thumb({
	root,
	theirs,
	file,
}: {
	root: string;
	theirs: boolean;
	file: string | null;
}) {
	const [src, setSrc] = useState("");

	useEffect(() => {
		if (file === null) {
			return;
		}
		let gone = false;
		let url = "";
		invoke<ArrayBuffer>("merge_cover", { root, theirs })
			.then((bytes) => {
				if (!gone) {
					url = URL.createObjectURL(new Blob([bytes]));
					setSrc(url);
				}
			})
			.catch(() => {
				// The file's name still says which cover it is.
			});
		return () => {
			gone = true;
			if (url !== "") {
				URL.revokeObjectURL(url);
			}
		};
	}, [root, theirs, file]);

	if (file === null) {
		return <span className="merge-spot__empty">No cover.</span>;
	}
	return (
		<span className="merge-card__cover">
			{src === "" ? (
				<span className="merge-card__thumb" />
			) : (
				<img className="merge-card__thumb" src={src} alt="" />
			)}
			<span className="merge-card__more">{file.split("/").pop()}</span>
		</span>
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
