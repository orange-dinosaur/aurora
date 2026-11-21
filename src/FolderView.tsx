import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties, ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import NameField from "./NameField";
import NewMenu from "./NewMenu";
import RowMenu from "./RowMenu";
import type { FolderNode, OverviewCard, ProjectDocument } from "./types";
import type { Making } from "./kinds";
import { documentPlaceholder, folderKinds, folderPlaceholder } from "./kinds";
import type { FolderRef } from "./tree";
import {
	band,
	counted,
	described,
	previewed,
	reached,
	summarised,
} from "./cards";
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
import { canTry } from "./takes";
import Target, { TargetField } from "./Target";
import type { FolderProgress } from "./bindings/FolderProgress";

/**
 * A card's face: a button that opens what it stands for, or, while its target
 * is typed, the same face without one, since a form cannot sit in a button.
 */
function Card({
	className,
	aiming,
	onClick,
	children,
}: {
	className: string;
	aiming: boolean;
	onClick: () => void;
	children: ReactNode;
}) {
	return aiming ? (
		<div className={className}>{children}</div>
	) : (
		<button type="button" className={className} onClick={onClick}>
			{children}
		</button>
	);
}

/**
 * The shape of a card's count against its target. The line above it is the
 * real report, and there is nothing to draw until a target is set.
 */
function bar(words: number, target: number | null) {
	const share = reached(words, target);
	if (share === null) {
		return null;
	}
	return (
		<span
			className="editor__progress card__progress"
			data-band={band(share)}
			style={{ "--progress": `${share * 100}%` } as CSSProperties}
		/>
	);
}

/** A card being named: the name goes straight onto it, with no box. */
function NamingCard({
	at,
	hint,
	children,
}: {
	at: number;
	hint: string;
	children: ReactNode;
}) {
	return (
		<div className="card card--naming">
			<span className="card__ordinal">
				{String(at + 1).padStart(2, "0")}
			</span>
			{children}
			<p className="card__hint">{hint}</p>
		</div>
	);
}

/** A card being retitled, on the item the card it replaces was in. */
function RenamingCard({
	at,
	name,
	placeholder,
	renaming,
	onSubmit,
	onCancel,
}: {
	at: number;
	name: string;
	placeholder: string;
	renaming: Renaming;
	onSubmit: (name: string) => void;
	onCancel: () => void;
}) {
	return (
		<li className="cards__item">
			<NamingCard at={at} hint="Enter to rename, Esc to cancel">
				<NameField
					label={`New name for ${name}`}
					placeholder={placeholder}
					initial={name}
					busy={renaming.kind === "saving"}
					error={
						renaming.kind === "refused" ? renaming.message : null
					}
					onSubmit={onSubmit}
					onCancel={onCancel}
				/>
			</NamingCard>
		</li>
	);
}

type Status = { kind: "idle" } | { kind: "error"; message: string };

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
	merging: boolean;
	onSelect: (document: ProjectDocument) => void;
	onTry: (document: ProjectDocument) => Promise<void>;
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
	/** Sets or clears (with null) this folder's word target. */
	onRetarget: (target: number | null) => Promise<void>;
	/** The same for a document or a folder shown as a card. */
	onRetargetDocument: (id: string, target: number | null) => Promise<void>;
	onRetargetFolder: (id: string, target: number | null) => Promise<void>;
};

export default function FolderView({
	root,
	folder,
	reload,
	merging,
	onSelect,
	onTry,
	onOpenFolder,
	onCreated,
	onRenamed,
	onFolderRenamed,
	onChanged,
	onMoved,
	onDelete,
	onDeleteFolder,
	onRetarget,
	onRetargetDocument,
	onRetargetFolder,
}: Props) {
	const [cards, setCards] = useState<OverviewCard[]>([]);
	// Worked out once per read rather than on every keystroke in a name field.
	const previews = useMemo(
		() =>
			new Map(
				cards.flatMap((card) =>
					card.node === "document"
						? [
								[
									card.id,
									previewed(card.front, card.excerpt),
								] as const,
							]
						: [],
				),
			),
		[cards],
	);
	const [target, setTarget] = useState<number | null>(null);
	// The card whose target is being typed, if one is.
	const [aiming, setAiming] = useState<string | null>(null);
	const [status, setStatus] = useState<Status>({ kind: "idle" });
	// A target refused is said in the alert line, and the field stays open.
	function aimed(done: Promise<void>) {
		return done.catch((error: unknown) => {
			setStatus({ kind: "error", message: failure(error).message });
			throw error;
		});
	}
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

	// Counts the reads, so only the newest is drawn. A reload says nothing
	// while it reads, so it cannot wipe a message or the field being typed in.
	const reads = useRef(0);
	const [loaded, setLoaded] = useState(false);
	const load = useCallback(async () => {
		reads.current += 1;
		const mine = reads.current;
		try {
			// The cards already add up to the folder's words; only its
			// target lives elsewhere.
			const [read, progress] = await Promise.all([
				invoke<OverviewCard[]>("folder_overview", {
					root,
					id: folder.id,
				}),
				invoke<FolderProgress>("folder_progress", {
					root,
					id: folder.id,
				}),
			]);
			if (mine !== reads.current) {
				return;
			}
			setCards(read);
			setTarget(progress.target);
		} catch (error) {
			if (mine === reads.current) {
				setStatus({ kind: "error", message: failure(error).message });
			}
		}
		if (mine === reads.current) {
			setLoaded(true);
		}
	}, [root, folder.id]);

	useEffect(() => {
		void load();
	}, [load, reload]);

	// A choice that already carries a name is made at once; the rest ask for one.
	function chosen(making: Making) {
		if (making.name) {
			void create(making, making.name);
		} else {
			setNaming({ kind: "open", making });
		}
	}

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
		setStatus({ kind: "idle" });
		try {
			await moveNode(root, id, parentId, index);
			onMoved();
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}

	async function remove(id: string, what: Making["what"]) {
		setStatus({ kind: "idle" });
		try {
			await (what === "folder" ? onDeleteFolder(id) : onDelete(id));
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}

	async function startTake(document: ProjectDocument) {
		setStatus({ kind: "idle" });
		try {
			await onTry(document);
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}

	if (!loaded) {
		return (
			<div className="overview overview--folder">
				<header className="overview__header">
					<h2 className="overview__title">{folder.name}</h2>
				</header>
				<p className="overview__note">Reading…</p>
			</div>
		);
	}

	const summary = summarised(cards);

	return (
		<div className="overview overview--folder">
			<header className="overview__header">
				<div className="overview__heading">
					{/* The folders above this one, which is what tells two
					    chapters of the same name apart. */}
					{folder.trail.length > 1 && (
						<p className="overview__where">
							{folder.trail.slice(0, -1).join(" · ")}
						</p>
					)}
					<h2 className="overview__title">{folder.name}</h2>
					{/* A div, not a paragraph: the words become a form while a
					    target is typed. */}
					<div className="overview__count">
						{summary.documents} ·{" "}
						<Target
							words={summary.words}
							target={target}
							onTarget={(target) => aimed(onRetarget(target))}
							merging={merging}
							inline
						/>
					</div>
				</div>
				<NewMenu
					label={`New in ${folder.name}`}
					kind={folder.kind}
					section={folder.section}
					held={folderKinds(cards)}
					text="New"
					className="overview__new"
					merging={merging}
					onChoose={chosen}
				/>
			</header>

			<ul className="cards">
				{cards.map((card, at) => {
					if (card.node === "folder") {
						return renaming.kind !== "closed" &&
							renaming.id === card.id ? (
							<RenamingCard
								key={card.id}
								at={at}
								name={card.name}
								placeholder={folderPlaceholder(card.kind)}
								renaming={renaming}
								onSubmit={(name) =>
									void rename(card.id, name, "folder")
								}
								onCancel={() => setRenaming({ kind: "closed" })}
							/>
						) : (
							<li
								key={card.id}
								className="cards__item"
								{...reorder.item(spot(card, at))}
								draggable={!merging && aiming !== card.id}
							>
								<Card
									className="card card--folder"
									aiming={aiming === card.id}
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
											card.target,
										)}
									</span>
									{aiming === card.id ? (
										<span className="card__meta">
											<TargetField
												words={card.words}
												target={card.target}
												onTarget={(target) =>
													aimed(
														onRetargetFolder(
															card.id,
															target,
														),
													)
												}
												onDone={() => setAiming(null)}
											/>
										</span>
									) : (
										card.target !== null && (
											<span className="card__meta">
												{counted(
													card.words,
													card.target,
												)}
											</span>
										)
									)}
									{bar(card.words, card.target)}
								</Card>
								<RowMenu
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
									onTarget={() => setAiming(card.id)}
									onDelete={() =>
										void remove(card.id, "folder")
									}
									merging={merging}
								/>
							</li>
						);
					}

					const document = card;
					return renaming.kind !== "closed" &&
						renaming.id === document.id ? (
						<RenamingCard
							key={document.id}
							at={at}
							name={document.title}
							placeholder={documentPlaceholder(folder.section)}
							renaming={renaming}
							onSubmit={(name) =>
								void rename(
									document.id,
									name,
									"document",
									document.title,
								)
							}
							onCancel={() => setRenaming({ kind: "closed" })}
						/>
					) : (
						<li
							key={document.id}
							className="cards__item"
							{...reorder.item(spot(document, at))}
							draggable={!merging && aiming !== document.id}
						>
							<Card
								className="card"
								aiming={aiming === document.id}
								onClick={() => onSelect(document)}
							>
								<span className="card__ordinal">
									{String(at + 1).padStart(2, "0")}
								</span>
								<span className="card__title">
									{document.title}
								</span>
								<span className="card__excerpt">
									{previews.get(document.id)}
								</span>
								<span className="card__meta">
									{aiming === document.id ? (
										<TargetField
											words={document.words}
											target={document.target}
											onTarget={(target) =>
												aimed(
													onRetargetDocument(
														document.id,
														target,
													),
												)
											}
											onDone={() => setAiming(null)}
										/>
									) : document.modified === null ? (
										"This document’s file is no longer there"
									) : (
										`${counted(document.words, document.target)} · ${when(document.modified)}`
									)}
								</span>
								{bar(document.words, document.target)}
							</Card>
							<RowMenu
								label={`Actions for ${document.title}`}
								root={root}
								ordering={{ index: at, count: cards.length }}
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
								onTarget={() => setAiming(document.id)}
								onDelete={() =>
									void remove(document.id, "document")
								}
								onTry={
									canTry(document.trail)
										? () => void startTake(document)
										: undefined
								}
								merging={merging}
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
							held={folderKinds(cards)}
							text="New"
							className="card card--new"
							merging={merging}
							onChoose={chosen}
						/>
					) : (
						<NamingCard
							at={cards.length}
							hint="Enter to add, Esc to cancel"
						>
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
						</NamingCard>
					)}
				</li>
			</ul>

			<p className="overview__message" role="alert">
				{status.kind === "error" ? status.message : ""}
			</p>
		</div>
	);
}
