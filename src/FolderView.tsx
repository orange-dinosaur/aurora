import { useCallback, useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import DocumentMenu from "./DocumentMenu";
import Icon from "./Icon";
import NameField from "./NameField";
import type { OverviewCard, ProjectDocument } from "./types";
import { counted, described, summarised } from "./cards";
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
	id: string;
	// The folder's own name, which the tab already knows. Only the cards are
	// worth a read.
	folder: string;
	// Changes when the project view has altered the manifest.
	reload: number;
	onSelect: (document: ProjectDocument) => void;
	onOpenFolder: (id: string, name: string) => void;
	onCreated: (document: ProjectDocument) => void;
	onRenamed: (document: ProjectDocument) => void;
	onReordered: () => void;
	// The project view owns this one: it has to write down what the writer
	// last typed before the file moves, and close the tab afterwards.
	onDelete: (id: string) => Promise<void>;
};

export default function FolderView({
	root,
	id,
	folder,
	reload,
	onSelect,
	onOpenFolder,
	onCreated,
	onRenamed,
	onReordered,
	onDelete,
}: Props) {
	const [cards, setCards] = useState<OverviewCard[]>([]);
	const [status, setStatus] = useState<Status>({ kind: "busy" });
	const [naming, setNaming] = useState<Naming>({ kind: "closed" });
	const [renaming, setRenaming] = useState<Renaming>({ kind: "closed" });
	const reorder = useReorder((id, index) => void move(id, index));

	const load = useCallback(async () => {
		setStatus({ kind: "busy" });
		try {
			setCards(
				await invoke<OverviewCard[]>("folder_overview", { root, id }),
			);
			setStatus({ kind: "idle" });
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}, [root, id]);

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

	if (status.kind === "busy" && cards.length === 0) {
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
					<p className="overview__count">{summarised(cards)}</p>
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
				{cards.map((card, at) => {
					if (card.node === "folder") {
						return (
							<li
								key={card.id}
								className="cards__item"
								{...reorder.item(card.id, at, id)}
							>
								<button
									type="button"
									className="card card--folder"
									onClick={() =>
										onOpenFolder(card.id, card.name)
									}
								>
									<span className="card__ordinal">
										{String(at + 1).padStart(2, "0")}
									</span>
									<span className="card__title">
										{card.name}
									</span>
									<span className="card__meta">
										{described(card.kind, card.children)}
									</span>
								</button>
							</li>
						);
					}

					const document = card;
					return renaming.kind !== "closed" &&
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
							{...reorder.item(document.id, at, id)}
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
								count={cards.length}
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
					);
				})}

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
