// The Versions tab: what changed since the last checkpoint, and the
// checkpoints of the open document or of the whole project, grouped by day.

import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { failure } from "./errors";
import {
	byDay,
	detail,
	lineKey,
	lines,
	type Line,
	type Overwrites,
} from "./lib/versions";
import type {
	Checkpoint,
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
		target: Overwrites,
	) => Promise<void>;
};

export default function Versions({
	root,
	page,
	scope,
	onScope,
	enclosing,
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
	const staged = status === null ? [] : lines(status.staged);
	const unstaged = status === null ? [] : lines(status.unstaged);
	const said =
		message.trim() !== "" ? message : first ? "First checkpoint" : "";
	const canAdd =
		!busy && said.trim() !== "" && staged.length + unstaged.length > 0;

	function addCheckpoint() {
		if (!canAdd) {
			return;
		}
		void run(async () => {
			await onBeforeKeep();
			await invoke("add_checkpoint", { root, message: said });
			setMessage("");
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
					</div>

					<Group
						heading="In the next checkpoint"
						all="Take all out"
						lines={staged}
						into={false}
						busy={busy}
						onMove={move}
						onCompare={compareLine}
						onPutBack={putBackLine}
					/>
					<Group
						heading="Changes"
						all="Add all"
						lines={unstaged}
						into={true}
						busy={busy}
						onMove={move}
						onCompare={compareLine}
						onPutBack={putBackLine}
					/>
					{status !== null &&
						staged.length + unstaged.length === 0 && (
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
									onSelect={() => setSelected(checkpoint.id)}
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
								/>
							))}
						</ul>
					</section>
				))}
		</div>
	);
}

// One group of the Changes list, hidden while it has no rows. `into` says
// which way its rows move: into the next checkpoint, or out of it.
function Group({
	heading,
	all,
	lines,
	into,
	busy,
	onMove,
	onCompare,
	onPutBack,
}: {
	heading: string;
	all: string;
	lines: Line[];
	into: boolean;
	busy: boolean;
	onMove: (row: StatusRow | null, into: boolean) => void;
	onCompare: (line: Line) => (() => void) | null;
	onPutBack: (line: Line) => (() => void) | null;
}) {
	if (lines.length === 0) {
		return null;
	}
	return (
		<section className="versions__day">
			<div className="changes__head">
				<h3 className="versions__date">{heading}</h3>
				<button
					type="button"
					className="versions__action"
					disabled={busy}
					onClick={() => onMove(null, into)}
				>
					{all}
				</button>
			</div>
			<ul className="versions__list">
				{lines.map((line) => {
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
								<button
									type="button"
									className="versions__action changes__back"
									disabled={busy}
									onClick={putBack}
								>
									Put back
								</button>
							)}
							<button
								type="button"
								className="changes__move"
								aria-label={
									into
										? "Add to the next checkpoint"
										: "Take out of the next checkpoint"
								}
								disabled={busy}
								onClick={() => onMove(line.row, into)}
							>
								{into ? "+" : "−"}
							</button>
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
}: {
	checkpoint: Checkpoint;
	selected: boolean;
	onSelect: () => void;
	/** Null while there is no document to act on. */
	onCompare: (() => void) | null;
	onPutBack: (() => void) | null;
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
				aria-pressed={selected}
				onClick={onSelect}
			>
				<span className="versions__mark" aria-hidden="true" />
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
					<button
						type="button"
						className="versions__action"
						disabled={onPutBack === null}
						onClick={onPutBack ?? undefined}
					>
						Put back
					</button>
				</span>
			)}
		</li>
	);
}
