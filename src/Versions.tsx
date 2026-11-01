// The Versions tab: what changed since the last checkpoint, and the
// checkpoints of the open document or of the whole project, grouped by day.

import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { failure } from "./errors";
import { BINDER } from "./lib/merge";
import Icon from "./Icon";
import PutBack from "./PutBack";
import {
	behind,
	byDay,
	changeList,
	detail,
	lineKey,
	settleBanner,
	settleFirst,
	settleLines,
	ticksLine,
	type Line,
	type MergeUnderway,
	type Overwrites,
	type SettleLine,
	type Tick,
} from "./lib/versions";
import type {
	Checkpoint,
	Piece,
	ProjectDocument,
	Status,
	StatusRow,
	VersionsScope,
} from "./types";

type Props = {
	root: string;
	/** The open document, or null while a folder overview is showing. */
	page: ProjectDocument | null;
	scope: VersionsScope;
	onScope: (scope: VersionsScope) => void;
	/** A repository the project sits inside, while the writer has not been told. */
	enclosing: string | null;
	/** The merge waiting to be settled, if there is one. */
	merge: MergeUnderway | null;
	/** Reads the merge again after a checkpoint or Abort merge ended it. */
	onMergeChanged: () => Promise<void>;
	/** Opens the merge screen on a conflicted document. */
	onSettle: (id: string) => void;
	/** Writes every open document to disk, so a version holds what is on screen. */
	onBeforeKeep: () => Promise<void>;
	/** Bumped when a session has been recorded. */
	logged: number;
	/** Opens compare at the checkpoint, on the given document when there is one. */
	onCompare: (checkpoint: Checkpoint, document?: string) => void;
	/** Writes everything down, runs the put back, and reloads what it changed. */
	/** Asks first when the target has changes not in a checkpoint. */
	onPutBack: (
		task: () => Promise<unknown>,
		target?: Overwrites,
	) => Promise<void>;
};

export default function Versions({
	root,
	page,
	scope,
	onScope,
	enclosing,
	merge,
	onMergeChanged,
	onSettle,
	onBeforeKeep,
	logged,
	onCompare,
	onPutBack,
}: Props) {
	const [checkpoints, setCheckpoints] = useState<Checkpoint[] | null>(null);
	const [selected, setSelected] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);
	// Bumped after anything here changes the history, to list it again.
	const [asked, setAsked] = useState(0);
	const [status, setStatus] = useState<Status | null>(null);
	const [message, setMessage] = useState("");
	// How many spots each conflicted document has, by id.
	const [spots, setSpots] = useState<Record<string, number>>({});
	const merging = merge?.merging ?? null;

	// The checkpoint that finishes a merge says so, until the writer changes it.
	useEffect(() => {
		if (merging !== null) {
			setMessage(merging.message);
		}
	}, [merging?.message]);

	useEffect(() => {
		if (merging === null) {
			setSpots({});
			return;
		}
		let current = true;
		const ids = merging.conflicts.flatMap((conflict) =>
			conflict.kind === "text" && conflict.id !== null
				? [conflict.id]
				: [],
		);
		Promise.all(
			ids.map(async (id) => {
				const pieces = await invoke<Piece[] | null>("merge_spots", {
					root,
					id,
				});
				const count = (pieces ?? []).filter(
					(piece) => piece.kind === "spot",
				).length;
				return [id, count] as const;
			}),
		)
			.then((counted) => {
				if (current) {
					setSpots(Object.fromEntries(counted));
				}
			})
			.catch((reason: unknown) => {
				if (current) {
					setError(failure(reason).message);
				}
			});
		return () => {
			current = false;
		};
	}, [root, merging]);

	// A folder overview has no document of its own to narrow to.
	const document = scope === "document" && page !== null ? page.id : null;

	useEffect(() => {
		let current = true;
		invoke<Checkpoint[]>("list_checkpoints", {
			root,
			version: null,
			document,
		})
			.then((listed) => {
				if (current) {
					setCheckpoints(listed);
					setError(null);
				}
			})
			.catch((reason: unknown) => {
				if (current) {
					setError(failure(reason).message);
				}
			});
		return () => {
			current = false;
		};
	}, [root, document, asked, logged]);

	useEffect(() => {
		if (scope !== "changes") {
			return;
		}
		let current = true;
		invoke<Status>("version_status", { root })
			.then((found) => {
				if (current) {
					setStatus(found);
				}
			})
			.catch((reason: unknown) => {
				if (current) {
					setError(failure(reason).message);
				}
			});
		return () => {
			current = false;
		};
	}, [root, scope, asked, logged]);

	// Shown once: from the next opening on, Rust stops reporting it.
	useEffect(() => {
		if (enclosing !== null) {
			void invoke("nested_notice_shown", { root });
		}
	}, [root, enclosing]);

	async function run(task: () => Promise<unknown>) {
		setBusy(true);
		try {
			await task();
			setError(null);
			setAsked((times) => times + 1);
		} catch (reason) {
			setError(failure(reason).message);
		} finally {
			setBusy(false);
		}
	}

	// This document puts back the open document; Whole project, everything.
	function putBack(checkpoint: Checkpoint) {
		const asked = { root, version: checkpoint.id };
		void run(() =>
			onPutBack(
				() =>
					document === null
						? invoke("put_back_everything", asked)
						: invoke("put_back_document", {
								...asked,
								id: document,
							}),
				document === null
					? { kind: "everything" }
					: { kind: "document", id: document },
			),
		);
	}

	const removes = (version: string, id: string) => () =>
		invoke<string[]>("put_back_removes", { root, version, id });
	const refuse = (reason: unknown) => setError(failure(reason).message);

	// A null row moves every row at once.
	function move(row: StatusRow | null, into: boolean) {
		void run(async () => {
			await onBeforeKeep();
			await (row === null
				? invoke(into ? "stage_all" : "unstage_all", { root })
				: invoke(into ? "stage_row" : "unstage_row", { root, row }));
		});
	}

	// The Changes list reads the whole project's history, whose newest entry is
	// the last checkpoint. None yet means the first is still to come.
	const head = checkpoints?.[0] ?? null;
	const first = checkpoints !== null && checkpoints.length === 0;
	const changes = status === null ? [] : changeList(status);
	const said =
		message.trim() !== "" ? message : first ? "First checkpoint" : "";
	const settle = merging === null ? [] : settleLines(merging, spots);
	// A merge is a checkpoint of its own even when it changed nothing here.
	const canAdd =
		!busy &&
		said.trim() !== "" &&
		settle.length === 0 &&
		(merging !== null || changes.length > 0);

	function addCheckpoint() {
		if (!canAdd) {
			return;
		}
		void run(async () => {
			await onBeforeKeep();
			// A tick means the change as it stands, not as it stood when ticked.
			if (merging === null) {
				const now = await invoke<Status>("version_status", { root });
				for (const row of behind(now)) {
					await invoke("stage_row", { root, row });
				}
			}
			await invoke("add_checkpoint", { root, message: said });
			setMessage("");
			if (merging !== null) {
				await onMergeChanged();
			}
		});
	}

	// Everything goes back to the last checkpoint, so the tabs catch up after.
	function abort() {
		void run(async () => {
			await onPutBack(() => invoke("abort_merge", { root }));
			setMessage("");
			await onMergeChanged();
		});
	}

	// Today's put back, from the last checkpoint. A folder comes back only
	// with everything, so it has none.
	function putBackLine(line: Line): (() => void) | null {
		if (head === null || line.change?.folder === true) {
			return null;
		}
		const asked = { root, version: head.id };
		const id = line.change?.id;
		return () =>
			void run(() =>
				onPutBack(
					() =>
						id === undefined
							? invoke("put_back_book", asked)
							: invoke("put_back_document", { ...asked, id }),
					id === undefined
						? { kind: "book" }
						: { kind: "document", id },
				),
			);
	}

	function removesLine(line: Line) {
		const id = line.change?.id;
		return head === null || id === undefined
			? undefined
			: removes(head.id, id);
	}

	// Only an edited document has words to compare.
	function compareLine(line: Line): (() => void) | null {
		const change = line.change;
		return head === null || change === null || !change.edited
			? null
			: () => onCompare(head, change.id);
	}

	const days = checkpoints === null ? [] : byDay(checkpoints, new Date());
	// This document with a folder overview showing has no document to act on.
	const noDocument = scope === "document" && document === null;

	return (
		<div className="versions">
			{enclosing !== null && (
				<p className="versions__notice">
					This project sits inside another git repository ({enclosing}
					). Aurora now keeps this project's history itself, so that
					repository will stop seeing changes inside it.
				</p>
			)}

			{/* First, so it stays put whichever list it shows. */}
			<div
				className="versions__scope"
				role="group"
				aria-label="Versions of"
			>
				<button
					type="button"
					className="versions__scope-button"
					aria-pressed={scope === "changes"}
					onClick={() => onScope("changes")}
				>
					Changes
				</button>
				<button
					type="button"
					className="versions__scope-button"
					aria-pressed={document !== null}
					disabled={page === null}
					onClick={() => onScope("document")}
				>
					This document
				</button>
				<button
					type="button"
					className="versions__scope-button"
					aria-pressed={scope !== "changes" && document === null}
					onClick={() => onScope("project")}
				>
					Whole project
				</button>
			</div>

			{/* Laid over the foot of the panel, so a message does not push
			    the list down. */}
			<p className="right-sidebar__trouble" role="alert">
				{error ?? ""}
			</p>

			{scope === "changes" && (
				<>
					{merge !== null && (
						<div className="merge-banner" role="status">
							<p className="merge-banner__title">
								Merging{" "}
								{merge.merging.name ?? "another version"} into{" "}
								{merge.into}
							</p>
							<p className="merge-banner__text">
								{settleBanner(settle.length)}
							</p>
							<button
								type="button"
								className="merge-banner__abort"
								disabled={busy}
								onClick={abort}
							>
								Abort merge
							</button>
						</div>
					)}

					<div className="changes__add">
						<textarea
							className="changes__message"
							aria-label="What changed"
							placeholder={
								first ? "First checkpoint" : "What changed"
							}
							rows={3}
							value={message}
							onChange={(event) => setMessage(event.target.value)}
							onKeyDown={(event) => {
								if (
									event.key === "Enter" &&
									(event.metaKey || event.ctrlKey)
								) {
									event.preventDefault();
									addCheckpoint();
								}
							}}
						/>
						<button
							type="button"
							className="versions__keep"
							disabled={!canAdd}
							onClick={addCheckpoint}
						>
							Add checkpoint
						</button>
						{/* Held open for the whole merge, so settling the last
						    row does not pull the list up. */}
						{merging !== null && (
							<p className="changes__why">
								{settle.length > 0
									? settleFirst(settle.length)
									: ""}
							</p>
						)}
					</div>

					{/* While merging, moving rows would stage over the
					    conflicts, so the rows stay where the merge put them. */}
					<Changes
						rows={changes}
						settle={settle}
						onSettle={onSettle}
						busy={busy || merging !== null}
						onMove={move}
						onCompare={compareLine}
						onPutBack={putBackLine}
						onRemoves={removesLine}
						onRefused={refuse}
					/>
					{status !== null &&
						merging === null &&
						changes.length === 0 && (
							<p className="right-sidebar__empty">
								Nothing has changed since the last checkpoint.
							</p>
						)}
				</>
			)}

			{scope !== "changes" &&
				checkpoints !== null &&
				checkpoints.length === 0 && (
					<p className="right-sidebar__empty">
						{document === null
							? "No checkpoints yet."
							: "No checkpoints of this document yet."}
					</p>
				)}

			{scope !== "changes" &&
				days.map((day) => (
					<section key={day.heading} className="versions__day">
						<h3 className="versions__date">{day.heading}</h3>
						<ul className="versions__list">
							{day.checkpoints.map((checkpoint) => (
								<Row
									key={checkpoint.id}
									checkpoint={checkpoint}
									selected={selected === checkpoint.id}
									onSelect={() =>
										setSelected(
											selected === checkpoint.id
												? null
												: checkpoint.id,
										)
									}
									onCompare={
										noDocument
											? null
											: () => onCompare(checkpoint)
									}
									onPutBack={
										noDocument || busy
											? null
											: () => putBack(checkpoint)
									}
									removes={
										document === null
											? undefined
											: removes(checkpoint.id, document)
									}
									onRefused={refuse}
								/>
							))}
						</ul>
					</section>
				))}
		</div>
	);
}

// The Changes list, hidden while it has no rows. A tick puts a row in the next
// checkpoint.
function Changes({
	rows,
	settle,
	onSettle,
	busy,
	onMove,
	onCompare,
	onPutBack,
	onRemoves,
	onRefused,
}: {
	rows: Tick[];
	/** What a merge left to settle, listed after the rows. */
	settle: SettleLine[];
	onSettle: (id: string) => void;
	busy: boolean;
	onMove: (row: StatusRow | null, into: boolean) => void;
	onCompare: (line: Line) => (() => void) | null;
	onPutBack: (line: Line) => (() => void) | null;
	onRemoves: (line: Line) => (() => Promise<string[]>) | undefined;
	onRefused: (reason: unknown) => void;
}) {
	if (rows.length + settle.length === 0) {
		return null;
	}
	const ticked = rows.filter((row) => row.ticked).length;
	const full = ticked === rows.length;

	return (
		<section className="versions__day">
			<div className="changes__head">
				<p className="changes__ticks">
					{ticksLine(ticked, rows.length)}
				</p>
				{rows.length > 0 && (
					<button
						type="button"
						className="versions__action"
						disabled={busy}
						onClick={() => onMove(null, !full)}
					>
						{full ? "Take all out" : "Add all"}
					</button>
				)}
			</div>
			<ul className="versions__list">
				{rows.map((line) => {
					const compare = onCompare(line);
					const putBack = onPutBack(line);
					const body = (
						<span className="versions__text">
							{line.name !== undefined && (
								<span className="versions__name">
									{line.name}
								</span>
							)}
							<span className="versions__detail">
								{line.what}
							</span>
						</span>
					);
					return (
						<li
							key={lineKey(line)}
							className="versions__item changes__row"
						>
							<button
								type="button"
								className="changes__tick"
								aria-label={`${line.name ?? line.what} in the next checkpoint`}
								aria-pressed={line.ticked}
								disabled={busy}
								onClick={() => onMove(line.row, !line.ticked)}
							>
								{line.ticked && <Icon name="check" />}
							</button>
							{compare === null ? (
								<div className="versions__pick changes__pick">
									{body}
								</div>
							) : (
								<button
									type="button"
									className="versions__pick changes__pick"
									onClick={compare}
								>
									{body}
								</button>
							)}
							{putBack !== null && (
								<PutBack
									className="versions__action changes__back"
									disabled={busy}
									removes={onRemoves(line)}
									onPutBack={putBack}
									onRefused={onRefused}
								>
									Put back
								</PutBack>
							)}
						</li>
					);
				})}
				{settle.map((line) => {
					const body = (
						<span className="versions__text">
							<span className="versions__name">{line.name}</span>
							<span className="versions__detail">
								{line.what}
							</span>
						</span>
					);
					const id = line.kind === "binder" ? BINDER : line.id;
					return (
						<li
							key={line.key}
							className="versions__item changes__row"
						>
							{id === null ? (
								<div className="versions__pick changes__pick">
									{body}
								</div>
							) : (
								<button
									type="button"
									className="versions__pick changes__pick"
									onClick={() => onSettle(id)}
								>
									{body}
								</button>
							)}
							<span className="changes__settle">To settle</span>
						</li>
					);
				})}
			</ul>
		</section>
	);
}

function Row({
	checkpoint,
	selected,
	onSelect,
	onCompare,
	onPutBack,
	removes,
	onRefused,
}: {
	checkpoint: Checkpoint;
	selected: boolean;
	onSelect: () => void;
	/** Null while there is no document to act on. */
	onCompare: (() => void) | null;
	onPutBack: (() => void) | null;
	removes?: () => Promise<string[]>;
	onRefused: (reason: unknown) => void;
}) {
	return (
		<li
			className={
				selected
					? "versions__item versions__item--selected"
					: "versions__item"
			}
		>
			<button
				type="button"
				className="versions__pick"
				aria-expanded={selected}
				onClick={onSelect}
			>
				<span className="versions__text">
					<span className="versions__name">{checkpoint.message}</span>
					<span className="versions__detail">
						{detail(checkpoint)}
					</span>
				</span>
			</button>
			{selected && (
				<span className="versions__actions">
					<button
						type="button"
						className="versions__action"
						disabled={onCompare === null}
						onClick={onCompare ?? undefined}
					>
						Compare
					</button>
					<PutBack
						className="versions__action"
						removes={removes}
						onPutBack={onPutBack}
						onRefused={onRefused}
					>
						Put back
					</PutBack>
				</span>
			)}
		</li>
	);
}
