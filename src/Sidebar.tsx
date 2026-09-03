import { Fragment, useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import DocumentRow from "./DocumentRow";
import FolderRow from "./FolderRow";
import Icon from "./Icon";
import NameField from "./NameField";
import NewMenu from "./NewMenu";
import type { FolderNode, ProjectDocument, TreeNode } from "./types";
import type { Making } from "./kinds";
import {
	createDocument,
	createFolder,
	moveNode,
	renameDocument,
	renameFolder,
} from "./documents";
import { failure } from "./errors";
import { useReorder } from "./reorder";
import type { Renaming } from "./rows";
import { indent, retitling } from "./rows";
import { documentsIn, rows, sections } from "./tree";
import type { FolderRef } from "./tree";

type Status =
	{ kind: "idle" } | { kind: "busy" } | { kind: "error"; message: string };

// At most one thing is being named at a time, so where it is going and what it
// is ride along with the state rather than sitting beside it.
type Naming =
	| { kind: "closed" }
	| { kind: "open"; at: string; making: Making }
	| { kind: "creating"; at: string; making: Making }
	| { kind: "refused"; at: string; making: Making; message: string };

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
	onFolderRenamed: (folder: FolderNode) => void;
	/** The manifest changed in a way every listing of it has to read again. */
	onChanged: () => void;
	/** The same, plus the paths of open documents may have changed under them. */
	onMoved: () => void;
	// The project view owns these two: it has to write down what the writer
	// last typed before the files move, and close the tabs afterwards.
	onDelete: (id: string) => Promise<void>;
	onDeleteFolder: (id: string) => Promise<void>;
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
	onFolderRenamed,
	onChanged,
	onMoved,
	onDelete,
	onDeleteFolder,
	onClose,
}: Props) {
	const [tree, setTree] = useState<TreeNode[]>([]);
	const [status, setStatus] = useState<Status>({ kind: "idle" });
	const [naming, setNaming] = useState<Naming>({ kind: "closed" });
	const [renaming, setRenaming] = useState<Renaming>({ kind: "closed" });
	// The folders drawn open. A project the writer has expanded nothing in
	// starts folded, so a deep manuscript opens as its parts rather than as
	// every scene in it.
	const [open, setOpen] = useState<ReadonlySet<string>>(() => new Set());
	const reorder = useReorder(
		(id, parentId, index) => void move(id, parentId, index),
	);

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

	// Which folders were open last time. A store that cannot be read leaves
	// the sidebar folded rather than stopping the project from opening.
	useEffect(() => {
		invoke<string[]>("read_expanded", { root })
			.then((ids) => setOpen(new Set(ids)))
			.catch(() => setOpen(new Set()));
	}, [root]);

	async function toggle(id: string) {
		const next = new Set(open);
		if (!next.delete(id)) {
			next.add(id);
		}
		setOpen(next);

		try {
			await invoke("write_expanded", { root, open: [...next] });
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
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

	async function rename(id: string, name: string, what: Making["what"]) {
		setRenaming({ kind: "saving", id });
		try {
			if (what === "folder") {
				onFolderRenamed(await renameFolder(root, id, name));
			} else {
				onRenamed(await renameDocument(root, id, name));
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

	// The field asking what to call the new thing, under whichever row asked
	// for it. Both a section header and a folder inside it want the same one.
	function field(at: string, where: string) {
		if (naming.kind === "closed") {
			return null;
		}

		return (
			<div className="sidebar__field">
				<NameField
					label={`Name of the new ${naming.making.noun} in ${where}`}
					placeholder={naming.making.placeholder}
					busy={naming.kind === "creating"}
					error={naming.kind === "refused" ? naming.message : null}
					onSubmit={(name) => void create(at, naming.making, name)}
					onCancel={() => setNaming({ kind: "closed" })}
				/>
			</div>
		);
	}

	return (
		<nav
			className={hidden ? "sidebar sidebar--hidden" : "sidebar"}
			aria-label="Documents"
		>
			<div className="sidebar__list">
				<div className="sidebar__sections">
					{sections(tree).map((section) => {
						const list = rows(section.children, section.id, open);

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
									naming.at === section.id &&
									field(section.id, section.name)}

								{list.length > 0 ? (
									<ul className="documents">
										{list.map((row) =>
											row.kind === "folder" ? (
												<Fragment key={row.id}>
													<FolderRow
														root={root}
														row={row}
														section={section.name}
														current={
															row.id ===
															selectedFolder
														}
														renaming={retitling(
															renaming,
															row.id,
														)}
														onOpen={() =>
															onOpenFolder({
																id: row.id,
																name: row.name,
																kind: row.folderKind,
																section:
																	section.name,
															})
														}
														onNew={(making) =>
															setNaming({
																kind: "open",
																at: row.id,
																making,
															})
														}
														onMove={(
															parentId,
															at,
														) =>
															void move(
																row.id,
																parentId,
																at,
															)
														}
														onRename={() =>
															setRenaming({
																kind: "open",
																id: row.id,
															})
														}
														onDelete={() =>
															void remove(
																row.id,
																"folder",
															)
														}
														onToggle={() =>
															void toggle(row.id)
														}
														onRetitle={(name) =>
															void rename(
																row.id,
																name,
																"folder",
															)
														}
														onCancelRetitle={() =>
															setRenaming({
																kind: "closed",
															})
														}
													/>
													{naming.kind !== "closed" &&
														naming.at ===
															row.id && (
															<li
																className="documents__item"
																style={indent(
																	row.depth +
																		1,
																)}
															>
																{field(
																	row.id,
																	row.name,
																)}
															</li>
														)}
												</Fragment>
											) : (
												<DocumentRow
													key={row.document.id}
													root={root}
													row={row}
													current={
														row.document.id ===
														selectedId
													}
													unsaved={unsaved.includes(
														row.document.id,
													)}
													renaming={retitling(
														renaming,
														row.document.id,
													)}
													drag={reorder.item(
														row.document.id,
														row.index,
														row.group,
													)}
													onOpen={() =>
														onSelect(row.document)
													}
													onMove={(parentId, to) =>
														void move(
															row.document.id,
															parentId,
															to,
														)
													}
													onRename={() =>
														setRenaming({
															kind: "open",
															id: row.document.id,
														})
													}
													onDelete={() =>
														void remove(
															row.document.id,
															"document",
														)
													}
													onRetitle={(name) =>
														void rename(
															row.document.id,
															name,
															"document",
														)
													}
													onCancelRetitle={() =>
														setRenaming({
															kind: "closed",
														})
													}
												/>
											),
										)}
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
