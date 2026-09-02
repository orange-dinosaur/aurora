import { useCallback, useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import DocumentMenu from "./DocumentMenu";
import Icon from "./Icon";
import NameField from "./NameField";
import type { DocumentSummary, ProjectDocument } from "./types";
import { createDocument, renameDocument, reorderDocument } from "./documents";
import { when } from "./dates";
import { useReorder } from "./reorder";
import { failure } from "./errors";

type Status =
	{ kind: "busy" } | { kind: "idle" } | { kind: "error"; message: string };

// The `+ New` card: a button until the writer presses it, then the field, and
// whatever Rust made of the last name they tried.
type Naming =
	| { kind: "closed" }
	| { kind: "open" }
	| { kind: "creating" }
	| { kind: "refused"; message: string };

// The card the writer is retitling, and what Rust made of the last name they
// tried.
type Renaming =
	| { kind: "closed" }
	| { kind: "open"; id: string }
	| { kind: "saving"; id: string }
	| { kind: "refused"; id: string; message: string };

type Props = {
	root: string;
	folder: string;
	// Changes when the project view has altered the manifest.
	reload: number;
	onSelect: (document: ProjectDocument) => void;
	onCreated: (document: ProjectDocument) => void;
	onRenamed: (document: ProjectDocument) => void;
	onReordered: () => void;
	// The project view owns this one: it has to write down what the writer
	// last typed before the file moves, and close the tab afterwards.
	onDelete: (id: string) => Promise<void>;
};

function counted(words: number, target: number | null) {
	if (target !== null) {
		return `${words.toLocaleString()} of ${target.toLocaleString()} words`;
	}
	return words === 1 ? "1 word" : `${words.toLocaleString()} words`;
}

// What the folder amounts to, for the line under its name.
function summarised(documents: DocumentSummary[]) {
	const words = documents.reduce((total, one) => total + one.words, 0);
	const kept =
		documents.length === 1 ? "1 document" : `${documents.length} documents`;
	return `${kept} · ${words.toLocaleString()} words`;
}

export default function SectionView({
	root,
	folder,
	reload,
	onSelect,
	onCreated,
	onRenamed,
	onReordered,
	onDelete,
}: Props) {
	const [documents, setDocuments] = useState<DocumentSummary[]>([]);
	const [status, setStatus] = useState<Status>({ kind: "busy" });
	const [naming, setNaming] = useState<Naming>({ kind: "closed" });
	const [renaming, setRenaming] = useState<Renaming>({ kind: "closed" });
	const reorder = useReorder((id, index) => void move(id, index));

	const load = useCallback(async () => {
		setStatus({ kind: "busy" });
		try {
			setDocuments(
				await invoke<DocumentSummary[]>("section_overview", {
					root,
					section: folder,
				}),
			);
			setStatus({ kind: "idle" });
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}, [root, folder]);

	useEffect(() => {
		void load();
	}, [load, reload]);

	async function create(name: string) {
		setNaming({ kind: "creating" });
		try {
			onCreated(await createDocument(root, folder, name));
			setNaming({ kind: "closed" });
		} catch (error) {
			// A name Rust would not take leaves the field open, holding what
			// was typed, so it can be corrected rather than typed again.
			setNaming({ kind: "refused", message: failure(error).message });
		}
	}

	async function rename(id: string, name: string) {
		setRenaming({ kind: "saving", id });
		try {
			onRenamed(await renameDocument(root, id, name));
			setRenaming({ kind: "closed" });
		} catch (error) {
			setRenaming({
				kind: "refused",
				id,
				message: failure(error).message,
			});
		}
	}

	async function move(id: string, index: number) {
		try {
			await reorderDocument(root, id, index);
			onReordered();
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}

	async function remove(id: string) {
		try {
			await onDelete(id);
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}

	if (status.kind === "busy" && documents.length === 0) {
		return (
			<div className="overview">
				<header className="overview__header">
					<h2 className="overview__title">{folder}</h2>
				</header>
				<p className="overview__note">Reading…</p>
			</div>
		);
	}

	return (
		<div className="overview">
			<header className="overview__header">
				<div className="overview__heading">
					<h2 className="overview__title">{folder}</h2>
					<p className="overview__count">{summarised(documents)}</p>
				</div>
				<button
					type="button"
					className="overview__new"
					onClick={() => setNaming({ kind: "open" })}
				>
					<Icon name="plus" />
					New document
				</button>
			</header>

			<ul className="cards">
				{documents.map((document, at) =>
					renaming.kind !== "closed" &&
					renaming.id === document.id ? (
						<li key={document.id} className="cards__item">
							<div className="card card--naming">
								<NameField
									label={`New name for ${document.title}`}
									placeholder="Chapter 2"
									initial={document.title}
									busy={renaming.kind === "saving"}
									error={
										renaming.kind === "refused"
											? renaming.message
											: null
									}
									onSubmit={(name) =>
										void rename(document.id, name)
									}
									onCancel={() =>
										setRenaming({ kind: "closed" })
									}
								/>
							</div>
						</li>
					) : (
						<li
							key={document.id}
							className="cards__item"
							{...reorder.item(document.id, at, folder)}
						>
							<button
								type="button"
								className="card"
								onClick={() => onSelect(document)}
							>
								<span className="card__ordinal">
									{String(at + 1).padStart(2, "0")}
								</span>
								<span className="card__title">
									{document.title}
								</span>
								<span className="card__excerpt">
									{document.excerpt}
								</span>
								<span className="card__meta">
									{document.modified === null
										? "This document’s file is no longer there"
										: `${counted(document.words, document.target)} · ${when(document.modified)}`}
								</span>
								{/* The line above is the real report; this is
								    only its shape, and there is nothing to
								    draw until a target is set. */}
								{document.target !== null && (
									<span
										className="editor__progress card__progress"
										style={
											{
												"--progress": `${Math.min(100, (document.words / document.target) * 100)}%`,
											} as CSSProperties
										}
									/>
								)}
							</button>
							<DocumentMenu
								label={`Actions for ${document.title}`}
								index={at}
								count={documents.length}
								onMove={(to) => void move(document.id, to)}
								onRename={() =>
									setRenaming({
										kind: "open",
										id: document.id,
									})
								}
								onDelete={() => void remove(document.id)}
							/>
						</li>
					),
				)}

				<li className="cards__item">
					{naming.kind === "closed" ? (
						<button
							type="button"
							className="card card--new"
							onClick={() => setNaming({ kind: "open" })}
						>
							+ New document
						</button>
					) : (
						<div className="card card--naming">
							<NameField
								label={`Name of the new document in ${folder}`}
								placeholder="Chapter 2"
								busy={naming.kind === "creating"}
								error={
									naming.kind === "refused"
										? naming.message
										: null
								}
								onSubmit={(name) => void create(name)}
								onCancel={() => setNaming({ kind: "closed" })}
							/>
						</div>
					)}
				</li>
			</ul>

			<p className="overview__message" role="alert">
				{status.kind === "error" ? status.message : ""}
			</p>
		</div>
	);
}
