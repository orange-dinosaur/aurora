// The Versions tab: keeping a version by name, and the history of the open
// document or of the whole project, grouped by day.

import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { failure } from "./errors";
import Icon from "./Icon";
import Menu, { MenuItem } from "./Menu";
import NameField from "./NameField";
import { byDay, detail } from "./lib/versions";
import type { Keep, ProjectDocument, Version, VersionsScope } from "./types";

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
	onCompare: (version: Version) => void;
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
}: Props) {
	const [versions, setVersions] = useState<Version[] | null>(null);
	const [selected, setSelected] = useState<string | null>(null);
	const [naming, setNaming] = useState<Naming | null>(null);
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);
	// Bumped after anything here changes the history, to list it again.
	const [asked, setAsked] = useState(0);

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

	const days = versions === null ? [] : byDay(versions, new Date());

	return (
		<div className="versions">
			{enclosing !== null && (
				<p className="versions__notice">
					This project sits inside another git repository ({enclosing}
					). Aurora now keeps this project's history itself, so that
					repository will stop seeing changes inside it.
				</p>
			)}

			{naming?.kind === "keep" ? (
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
					className="versions__keep"
					onClick={() => setNaming({ kind: "keep" })}
				>
					<Icon name="plus" />
					New version
				</button>
			)}

			<div
				className="versions__scope"
				role="group"
				aria-label="Versions of"
			>
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
					aria-pressed={document === null}
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

			{versions !== null && versions.length === 0 && (
				<p className="right-sidebar__empty">
					No versions of this document yet.
				</p>
			)}

			{days.map((day) => (
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
										initial={version.name ?? ""}
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
										setNaming({ kind: "rename", version })
									}
									onUnname={() => unname(version)}
									onCompare={
										scope === "document" &&
										document === null
											? null
											: () => onCompare(version)
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

function Row({
	version,
	selected,
	onSelect,
	onRename,
	onUnname,
	onCompare,
}: {
	version: Version;
	selected: boolean;
	onSelect: () => void;
	onRename: () => void;
	onUnname: () => void;
	/** Null while the whole project is listed. */
	onCompare: (() => void) | null;
}) {
	const named = version.name !== null;
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
						<span className="versions__name">{version.name}</span>
						{version.words !== null && (
							<span className="versions__words">
								{version.words.toLocaleString()}
							</span>
						)}
					</>
				) : (
					<span className="versions__text">
						{named && (
							<span className="versions__name">
								{version.name}
							</span>
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
					<button type="button" className="versions__action" disabled>
						Put back
					</button>
				</span>
			)}
		</li>
	);
}
