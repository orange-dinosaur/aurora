import { useCallback, useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import DocumentMenu from "./DocumentMenu";
import FolderMenu from "./FolderMenu";
import NameField from "./NameField";
import NewMenu from "./NewMenu";
import type { FolderNode, OverviewCard, ProjectDocument } from "./types";
import type { Making } from "./kinds";
import { folderPlaceholder } from "./kinds";
import type { FolderRef } from "./tree";
import { counted, described, previewed, summarised } from "./cards";
import {
	createDocument,
	createFolder,
	moveNode,
	renameDocument,
	renameFolder,
} from "./documents";
import { when } from "./dates";
import type { Spot } from "./reorder";
import { useReorder } from "./reorder";
import { failure } from "./errors";

type Status =
	{ kind: "busy" } | { kind: "idle" } | { kind: "error"; message: string };

// The `+ New` card: a menu until the writer chooses from it, then the field
// asking what to call what they chose, and whatever Rust made of the last name
// they tried.
type Naming =
	| { kind: "closed" }
	| { kind: "open"; making: Making }
	| { kind: "creating"; making: Making }
	| { kind: "refused"; making: Making; message: string };

// The card the writer is retitling, and what Rust made of the last name they
// tried. Whether it is a document or a folder rides with the card that opened
// the field rather than with the state, since only that card draws it.
type Renaming =
	| { kind: "closed" }
	| { kind: "open"; id: string }
	| { kind: "saving"; id: string }
	| { kind: "refused"; id: string; message: string };

type Props = {
	root: string;
	// What the tab already knows about the folder. Only the cards are worth a
	// read.
	folder: FolderRef;
	// Changes when the project view has altered the manifest.
	reload: number;
	onSelect: (document: ProjectDocument) => void;
	onOpenFolder: (folder: FolderRef) => void;
	onCreated: (document: ProjectDocument) => void;
	/** The renamed document, and the title it answered to before. */
	onRenamed: (document: ProjectDocument, was: string) => void;
	onFolderRenamed: (folder: FolderNode) => void;
	/** The manifest changed in a way every listing of it has to read again. */
	onChanged: () => void;
	/** The same, plus the paths of open documents may have changed under them. */
	onMoved: () => void;
	// The project view owns these two: it has to write down what the writer
	// last typed before the files move, and close the tabs afterwards.
	onDelete: (id: string) => Promise<void>;
	onDeleteFolder: (id: string) => Promise<void>;
};

export default function FolderView({
	root,
	folder,
	reload,
	onSelect,
	onOpenFolder,
	onCreated,
	onRenamed,
	onFolderRenamed,
	onChanged,
	onMoved,
	onDelete,
	onDeleteFolder,
}: Props) {
	const [cards, setCards] = useState<OverviewCard[]>([]);
	const [status, setStatus] = useState<Status>({ kind: "busy" });
	const [naming, setNaming] = useState<Naming>({ kind: "closed" });
	const [renaming, setRenaming] = useState<Renaming>({ kind: "closed" });
	// The cards flow across the overview rather than down it, so a landing
	// beside one is to its left or its right.
	const reorder = useReorder(
		(id, parentId, index) => void move(id, parentId, index),
		"x",
	);

	// What one card offers a drag. Everything on an overview sits in the folder
	// it is showing, so they all share a group, and nothing a card holds is
	// drawn here for a drag to get lost inside.
	function spot(card: OverviewCard, at: number): Spot {
		return {
			id: card.id,
			at,
			group: folder.id,
			section: folder.section,
			kind: card.node === "folder" ? card.kind : null,
			folder: card.node === "folder",
			holds: card.node === "folder" ? card.children : 0,
			within: false,
		};
	}

	const load = useCallback(async () => {
		setStatus({ kind: "busy" });
		try {
			setCards(
				await invoke<OverviewCard[]>("folder_overview", {
					root,
					id: folder.id,
				}),
			);
			setStatus({ kind: "idle" });
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}, [root, folder.id]);

	useEffect(() => {
		void load();
	}, [load, reload]);

	async function create(making: Making, name: string) {
		setNaming({ kind: "creating", making });
		try {
			if (making.what === "document") {
				onCreated(await createDocument(root, folder.id, name));
			} else {
				await createFolder(root, folder.id, name, making.kind);
				// Nothing to open: a new folder is empty, so the listings
				// simply read it again.
				onChanged();
			}
			setNaming({ kind: "closed" });
		} catch (error) {
			// A name Rust would not take leaves the field open, holding what
			// was typed, so it can be corrected rather than typed again.
			setNaming({
				kind: "refused",
				making,
				message: failure(error).message,
			});
		}
	}

	async function rename(
		id: string,
		name: string,
		what: Making["what"],
		was = "",
	) {
		setRenaming({ kind: "saving", id });
		try {
			if (what === "folder") {
				onFolderRenamed(await renameFolder(root, id, name));
			} else {
				onRenamed(await renameDocument(root, id, name), was);
			}
			setRenaming({ kind: "closed" });
		} catch (error) {
			setRenaming({
				kind: "refused",
				id,
				message: failure(error).message,
			});
		}
	}

	async function move(id: string, parentId: string, index: number) {
		try {
			await moveNode(root, id, parentId, index);
			onMoved();
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}

	async function remove(id: string, what: Making["what"]) {
		try {
			await (what === "folder" ? onDeleteFolder(id) : onDelete(id));
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}

	if (status.kind === "busy" && cards.length === 0) {
		return (
			<div className="overview">
				<header className="overview__header">
					<h2 className="overview__title">{folder.name}</h2>
				</header>
				<p className="overview__note">Reading…</p>
			</div>
		);
	}

	return (
		<div className="overview">
			<header className="overview__header">
				<div className="overview__heading">
					<h2 className="overview__title">{folder.name}</h2>
					<p className="overview__count">{summarised(cards)}</p>
				</div>
				<NewMenu
					label={`New in ${folder.name}`}
					kind={folder.kind}
					section={folder.section}
					text="New"
					className="overview__new"
					onChoose={(making) => setNaming({ kind: "open", making })}
				/>
			</header>

			<ul className="cards">
				{cards.map((card, at) => {
					if (card.node === "folder") {
						return renaming.kind !== "closed" &&
							renaming.id === card.id ? (
							<li key={card.id} className="cards__item">
								<div className="card card--naming">
									<NameField
										label={`New name for ${card.name}`}
										placeholder={folderPlaceholder(
											card.kind,
										)}
										initial={card.name}
										busy={renaming.kind === "saving"}
										error={
											renaming.kind === "refused"
												? renaming.message
												: null
										}
										onSubmit={(name) =>
											void rename(card.id, name, "folder")
										}
										onCancel={() =>
											setRenaming({ kind: "closed" })
										}
									/>
								</div>
							</li>
						) : (
							<li
								key={card.id}
								className="cards__item"
								{...reorder.item(spot(card, at))}
							>
								<button
									type="button"
									className="card card--folder"
									onClick={() =>
										onOpenFolder({
											id: card.id,
											name: card.name,
											kind: card.kind,
											section: folder.section,
											trail: [...folder.trail, card.name],
										})
									}
								>
									<span className="card__ordinal">
										{String(at + 1).padStart(2, "0")}
									</span>
									<span className="card__title">
										{card.name}
									</span>
									<span className="card__meta">
										{described(
											card.kind,
											card.children,
											card.words,
										)}
									</span>
								</button>
								<FolderMenu
									label={`Actions for ${card.name}`}
									root={root}
									moving={{
										id: card.id,
										kind: card.kind,
										folder: true,
										from: folder.id,
									}}
									onMove={(parentId, to) =>
										void move(card.id, parentId, to)
									}
									onRename={() =>
										setRenaming({
											kind: "open",
											id: card.id,
										})
									}
									onDelete={() =>
										void remove(card.id, "folder")
									}
								/>
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
										void rename(
											document.id,
											name,
											"document",
											document.title,
										)
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
							{...reorder.item(spot(document, at))}
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
									{previewed(
										document.front,
										document.excerpt,
									)}
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
								root={root}
								index={at}
								count={cards.length}
								moving={{
									id: document.id,
									kind: null,
									folder: false,
									from: folder.id,
								}}
								onMove={(parentId, to) =>
									void move(document.id, parentId, to)
								}
								onRename={() =>
									setRenaming({
										kind: "open",
										id: document.id,
									})
								}
								onDelete={() =>
									void remove(document.id, "document")
								}
							/>
						</li>
					);
				})}

				<li className="cards__item cards__item--new">
					{naming.kind === "closed" ? (
						<NewMenu
							label={`New in ${folder.name}`}
							kind={folder.kind}
							section={folder.section}
							text="New"
							className="card card--new"
							onChoose={(making) =>
								setNaming({ kind: "open", making })
							}
						/>
					) : (
						<div className="card card--naming">
							<NameField
								label={`Name of the new ${naming.making.noun} in ${folder.name}`}
								placeholder={naming.making.placeholder}
								busy={naming.kind === "creating"}
								error={
									naming.kind === "refused"
										? naming.message
										: null
								}
								onSubmit={(name) =>
									void create(naming.making, name)
								}
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
