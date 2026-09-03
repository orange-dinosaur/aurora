import { useCallback, useEffect, useState } from "react";
import type { CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import DocumentMenu from "./DocumentMenu";
import Icon from "./Icon";
import NameField from "./NameField";
import NewMenu from "./NewMenu";
import type { ProjectDocument, TreeNode } from "./types";
import type { Making } from "./kinds";
import {
	createDocument,
	createFolder,
	renameDocument,
	reorderDocument,
} from "./documents";
import { failure } from "./errors";
import { useReorder } from "./reorder";
import { documentsIn, rows, sections } from "./tree";
import type { FolderRef } from "./tree";

// How far in a row sits, as the stylesheet reads it.
function indent(depth: number) {
	return { "--depth": depth } as CSSProperties;
}

type Status =
	{ kind: "idle" } | { kind: "busy" } | { kind: "error"; message: string };

// At most one thing is being named at a time, so where it is going and what it
// is ride along with the state rather than sitting beside it.
type Naming =
	| { kind: "closed" }
	| { kind: "open"; at: string; making: Making }
	| { kind: "creating"; at: string; making: Making }
	| { kind: "refused"; at: string; making: Making; message: string };

// The document the writer is retitling, and what Rust made of the last name
// they tried.
type Renaming =
	| { kind: "closed" }
	| { kind: "open"; id: string }
	| { kind: "saving"; id: string }
	| { kind: "refused"; id: string; message: string };

type Props = {
	// Styled out rather than unmounted, so a half-typed section name and the
	// sections already read are still there when it comes back.
	hidden: boolean;
	root: string;
	// Changes when the project view has altered the manifest.
	reload: number;
	// The open documents with unwritten changes. The project view already
	// knows which those are; working it out again here would mean a second
	// answer that could disagree with the tab strip.
	unsaved: string[];
	selectedId: string | null;
	/** The id of the folder whose overview is showing, if one is. */
	selectedFolder: string | null;
	selectedTrash: boolean;
	onSelect: (document: ProjectDocument) => void;
	onOpenFolder: (folder: FolderRef) => void;
	onOpenTrash: () => void;
	onCreated: (document: ProjectDocument) => void;
	onRenamed: (document: ProjectDocument) => void;
	/** The manifest changed in a way every listing of it has to read again. */
	onChanged: () => void;
	// The project view owns this one: it has to write down what the writer
	// last typed before the file moves, and close the tab afterwards.
	onDelete: (id: string) => Promise<void>;
	onClose: () => void;
};

export default function Sidebar({
	hidden,
	root,
	reload,
	unsaved,
	selectedId,
	selectedFolder,
	selectedTrash,
	onSelect,
	onOpenFolder,
	onOpenTrash,
	onCreated,
	onRenamed,
	onChanged,
	onDelete,
	onClose,
}: Props) {
	const [tree, setTree] = useState<TreeNode[]>([]);
	const [status, setStatus] = useState<Status>({ kind: "idle" });
	const [naming, setNaming] = useState<Naming>({ kind: "closed" });
	const [renaming, setRenaming] = useState<Renaming>({ kind: "closed" });
	const reorder = useReorder((id, index) => void move(id, index));

	// Reads the manifest as it stands. Looking at the folder again is the
	// titlebar's refresh, which bumps `reload` once it has done so.
	const load = useCallback(async () => {
		setStatus({ kind: "busy" });
		try {
			setTree(await invoke<TreeNode[]>("document_tree", { root }));
			setStatus({ kind: "idle" });
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}, [root]);

	useEffect(() => {
		void load();
	}, [load, reload]);

	async function move(id: string, index: number) {
		try {
			await reorderDocument(root, id, index);
			onChanged();
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

	async function create(at: string, making: Making, name: string) {
		setNaming({ kind: "creating", at, making });
		try {
			if (making.what === "document") {
				onCreated(await createDocument(root, at, name));
			} else {
				await createFolder(root, at, name, making.kind);
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
				at,
				making,
				message: failure(error).message,
			});
		}
	}

	return (
		<nav
			className={hidden ? "sidebar sidebar--hidden" : "sidebar"}
			aria-label="Documents"
		>
			<div className="sidebar__list">
				<div className="sidebar__sections">
					{sections(tree).map((section) => {
						const list = rows(section.children, section.id);

						return (
							<section key={section.id} className="section">
								<div className="section__header">
									<h3 className="section__title">
										<button
											type="button"
											className="section__open"
											aria-current={
												section.id === selectedFolder
													? "page"
													: undefined
											}
											onClick={() =>
												onOpenFolder({
													id: section.id,
													name: section.name,
													kind: section.kind,
													section: section.name,
												})
											}
										>
											{section.name}
										</button>
									</h3>
									<span className="section__count">
										{documentsIn(section.children)}
									</span>
									<NewMenu
										label={`New in ${section.name}`}
										kind={section.kind}
										section={section.name}
										onChoose={(making) =>
											setNaming({
												kind: "open",
												at: section.id,
												making,
											})
										}
									/>
								</div>

								{naming.kind !== "closed" &&
									naming.at === section.id && (
										<div className="sidebar__field">
											<NameField
												label={`Name of the new ${naming.making.noun} in ${section.name}`}
												placeholder={
													naming.making.placeholder
												}
												busy={
													naming.kind === "creating"
												}
												error={
													naming.kind === "refused"
														? naming.message
														: null
												}
												onSubmit={(name) =>
													void create(
														section.id,
														naming.making,
														name,
													)
												}
												onCancel={() =>
													setNaming({
														kind: "closed",
													})
												}
											/>
										</div>
									)}
								{list.length > 0 ? (
									<ul className="documents">
										{list.map((row) => {
											if (row.kind === "folder") {
												return (
													<li
														key={row.id}
														className="documents__item"
														style={indent(
															row.depth,
														)}
													>
														<button
															type="button"
															className="folder"
															aria-current={
																row.id ===
																selectedFolder
																	? "page"
																	: undefined
															}
															onClick={() =>
																onOpenFolder({
																	id: row.id,
																	name: row.name,
																	kind: row.folderKind,
																	section:
																		section.name,
																})
															}
														>
															{row.name}
														</button>
													</li>
												);
											}

											const doc = row.document;
											return renaming.kind !== "closed" &&
												renaming.id === doc.id ? (
												<li
													key={doc.id}
													className="documents__item"
													style={indent(row.depth)}
												>
													<div className="sidebar__field">
														<NameField
															label={`New name for ${doc.title}`}
															placeholder="Chapter 2"
															initial={doc.title}
															busy={
																renaming.kind ===
																"saving"
															}
															error={
																renaming.kind ===
																"refused"
																	? renaming.message
																	: null
															}
															onSubmit={(name) =>
																void rename(
																	doc.id,
																	name,
																)
															}
															onCancel={() =>
																setRenaming({
																	kind: "closed",
																})
															}
														/>
													</div>
												</li>
											) : (
												<li
													key={doc.id}
													className="documents__item"
													style={indent(row.depth)}
													{...reorder.item(
														doc.id,
														row.index,
														row.group,
													)}
												>
													<button
														type="button"
														className="document"
														aria-current={
															doc.id ===
															selectedId
																? "page"
																: undefined
														}
														onClick={() =>
															onSelect(doc)
														}
													>
														<span className="document__at">
															{String(
																row.index + 1,
															).padStart(2, "0")}
														</span>
														<span className="document__title">
															{doc.title}
														</span>
														{/* Always in the row and
														    faded when there is
														    nothing to say, so the
														    title never shifts as
														    the writer types. */}
														<span
															className="document__dirty"
															data-dirty={
																unsaved.includes(
																	doc.id,
																)
																	? ""
																	: undefined
															}
															aria-hidden="true"
														/>
													</button>
													<DocumentMenu
														label={`Actions for ${doc.title}`}
														index={row.index}
														count={row.siblings}
														onMove={(to) =>
															void move(
																doc.id,
																to,
															)
														}
														onRename={() =>
															setRenaming({
																kind: "open",
																id: doc.id,
															})
														}
														onDelete={() =>
															void remove(doc.id)
														}
													/>
												</li>
											);
										})}
									</ul>
								) : (
									<p className="section__empty">
										Nothing here yet
									</p>
								)}
							</section>
						);
					})}
				</div>

				<p className="sidebar__message" role="alert">
					{status.kind === "error" ? status.message : ""}
				</p>
			</div>

			<div className="sidebar__foot">
				<button
					type="button"
					className="sidebar__leave"
					aria-current={selectedTrash ? "page" : undefined}
					onClick={onOpenTrash}
				>
					<Icon name="trash" />
					Trash
				</button>
				<button
					type="button"
					className="sidebar__leave"
					onClick={onClose}
				>
					<Icon name="log-out" />
					Close project
				</button>
			</div>
		</nav>
	);
}
