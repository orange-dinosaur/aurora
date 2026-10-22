// The Versions tab: what changed since the last checkpoint, keeping a version
// by name, and the history of the open document or of the whole project,
// grouped by day.

import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { failure } from "./errors";
import Icon from "./Icon";
import Menu, { MenuItem } from "./Menu";
import NameField from "./NameField";
import {
	byDay,
	detail,
	label,
	lineKey,
	lines,
	ownName,
	type Line,
} from "./lib/versions";
import type {
	Keep,
	ProjectDocument,
	Status,
	StatusRow,
	Version,
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
	/** Bumped when a session has kept a version of its own. */
	logged: number;
	/** Opens compare at the version, on the given document when there is one. */
	onCompare: (version: Version, document?: string) => void;
	/** Writes everything down, runs the put back, and reloads what it changed. */
	onPutBack: (task: () => Promise<unknown>) => Promise<void>;
};

// Which name field is open: the one for a new version, or one over a row.
type Naming = { kind: "keep" } | { kind: "rename"; version: Version };

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
	const [versions, setVersions] = useState<Version[] | null>(null);
	const [selected, setSelected] = useState<string | null>(null);
	const [naming, setNaming] = useState<Naming | null>(null);
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
		invoke<Version[]>("list_versions", { root, document })
			.then((listed) => {
				if (current) {
					setVersions(listed);
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
			setNaming(null);
			setError(null);
			setAsked((times) => times + 1);
		} catch (reason) {
			setError(failure(reason).message);
		} finally {
			setBusy(false);
		}
	}

	function keep(name: string) {
		const named: Keep = { kind: "named", name };
		void run(async () => {
			await onBeforeKeep();
			await invoke("keep_version", { root, keep: named });
		});
	}

	function rename(version: Version, name: string) {
		void run(() =>
			invoke("rename_version", { root, id: version.id, name }),
		);
	}

	function unname(version: Version) {
		void run(() => invoke("unname_version", { root, id: version.id }));
	}

	// This document puts back the open document; Whole project, everything.
	function putBack(version: Version) {
		const asked = { root, version: version.id, label: label(version) };
		void run(() =>
			onPutBack(() =>
				document === null
					? invoke("put_back_everything", asked)
					: invoke("put_back_document", { ...asked, id: document }),
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

	// The Changes list sits beside the whole project's history, whose newest
	// entry is the last checkpoint. None yet means the first is still to come.
	const head = versions?.[0] ?? null;
	const first = versions !== null && versions.length === 0;
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
		const asked = { root, version: head.id, label: label(head) };
		const id = line.change?.id;
		return () =>
			void run(() =>
				onPutBack(() =>
					id === undefined
						? invoke("put_back_book", asked)
						: invoke("put_back_document", { ...asked, id }),
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

	const days = versions === null ? [] : byDay(versions, new Date());
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
				{naming === null ? (error ?? "") : ""}
			</p>

			{scope === "changes" ? (
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
			) : naming?.kind === "keep" ? (
				<div className="versions__field">
					<NameField
						label="Name of the version"
						placeholder="Name this version"
						busy={busy}
						working="Keeping…"
						error={error}
						onSubmit={keep}
						onCancel={() => {
							setNaming(null);
							setError(null);
						}}
					/>
				</div>
			) : (
				<button
					type="button"
					className="versions__keep versions__new"
					onClick={() => setNaming({ kind: "keep" })}
				>
					<Icon name="plus" />
					New version
				</button>
			)}

			{scope !== "changes" &&
				versions !== null &&
				versions.length === 0 && (
					<p className="right-sidebar__empty">
						No versions of this document yet.
					</p>
				)}

			{scope !== "changes" &&
				days.map((day) => (
					<section key={day.heading} className="versions__day">
						<h3 className="versions__date">{day.heading}</h3>
						<ul className="versions__list">
							{day.versions.map((version) =>
								naming?.kind === "rename" &&
								naming.version.id === version.id ? (
									<li
										key={version.id}
										className="versions__item versions__field"
									>
										<NameField
											label="Name of the version"
											placeholder="Name this version"
											initial={ownName(version) ?? ""}
											busy={busy}
											working="Renaming…"
											error={error}
											onSubmit={(name) =>
												rename(version, name)
											}
											onCancel={() => {
												setNaming(null);
												setError(null);
											}}
										/>
									</li>
								) : (
									<Row
										key={version.id}
										version={version}
										selected={selected === version.id}
										onSelect={() => setSelected(version.id)}
										onRename={() =>
											setNaming({
												kind: "rename",
												version,
											})
										}
										onUnname={() => unname(version)}
										onCompare={
											noDocument
												? null
												: () => onCompare(version)
										}
										onPutBack={
											noDocument || busy
												? null
												: () => putBack(version)
										}
									/>
								),
							)}
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
	version,
	selected,
	onSelect,
	onRename,
	onUnname,
	onCompare,
	onPutBack,
}: {
	version: Version;
	selected: boolean;
	onSelect: () => void;
	onRename: () => void;
	onUnname: () => void;
	/** Null while there is no document to act on. */
	onCompare: (() => void) | null;
	onPutBack: (() => void) | null;
}) {
	const name = ownName(version);
	const named = name !== null;
	const className = [
		"versions__item",
		named ? "versions__item--named" : "",
		selected ? "versions__item--selected" : "",
	]
		.filter((part) => part !== "")
		.join(" ");

	return (
		<li className={className}>
			<button
				type="button"
				className="versions__pick"
				aria-pressed={selected}
				onClick={onSelect}
				onDoubleClick={() => {
					if (named) {
						onRename();
					}
				}}
			>
				<span className="versions__mark" aria-hidden="true" />
				{/* A named version at rest is its name and its words; the rest of
				    what is known about it opens with it. */}
				{named && !selected ? (
					<>
						<span className="versions__name">{name}</span>
						{version.words !== null && (
							<span className="versions__words">
								{version.words.toLocaleString()}
							</span>
						)}
					</>
				) : (
					<span className="versions__text">
						{named && (
							<span className="versions__name">{name}</span>
						)}
						<span className="versions__detail">
							{detail(version)}
						</span>
					</span>
				)}
			</button>
			<Menu label="Version actions" icon="more-vertical">
				{(close) => (
					<>
						<MenuItem
							onSelect={() => {
								close();
								onRename();
							}}
						>
							{named ? "Rename" : "Name this version"}
						</MenuItem>
						{named && (
							<MenuItem
								onSelect={() => {
									close();
									onUnname();
								}}
							>
								Remove name
							</MenuItem>
						)}
					</>
				)}
			</Menu>
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
