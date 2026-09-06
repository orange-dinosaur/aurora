import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import DocumentRow from "./DocumentRow";
import FolderRow from "./FolderRow";
import Grip from "./Grip";
import Icon from "./Icon";
import { SIDEBAR } from "./panels";
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
import { FOLD, pressed } from "./formatting";
import { useReorder } from "./reorder";
import type { Renaming } from "./rows";
import { indent, retitling, spot } from "./rows";
import { timed } from "./timing";
import { documentsIn, folded, inside, rows, sections, wordsIn } from "./tree";
import type { FolderRef, Row } from "./tree";

/** Nothing is being dragged, so no row is inside anything. */
const NONE: ReadonlySet<string> = new Set();

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
	/** How wide the writer has dragged it, in pixels. */
	width: number;
	onWidth: (width: number) => void;
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
	/** Opens the page about a character or a place, for the rows that are one. */
	onSubject: (document: ProjectDocument) => void;
	onOpenFolder: (folder: FolderRef) => void;
	onOpenTrash: () => void;
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
	/**
	 * How many words the project holds, said again every time the tree is read.
	 * This is the only place that count is going anyway, so nothing else has to
	 * open every file to learn it. Has to be stable, or the tree is read again
	 * on every render.
	 */
	onWords: (words: number) => void;
	onDelete: (id: string) => Promise<void>;
	onDeleteFolder: (id: string) => Promise<void>;
	onClose: () => void;
};

export default function Sidebar({
	hidden,
	width,
	onWidth,
	root,
	reload,
	unsaved,
	selectedId,
	selectedFolder,
	selectedTrash,
	onSelect,
	onSubject,
	onOpenFolder,
	onOpenTrash,
	onCreated,
	onRenamed,
	onFolderRenamed,
	onChanged,
	onMoved,
	onWords,
	onDelete,
	onDeleteFolder,
	onClose,
}: Props) {
	// Held here rather than passed in, so the element the grip moves is the
	// one this component drew.
	const panel = useRef<HTMLElement>(null);
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
			const read = await timed(
				"document tree",
				invoke<TreeNode[]>("document_tree", { root }),
			);
			setTree(read);
			onWords(wordsIn(read));
			setStatus({ kind: "idle" });
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}, [root, onWords]);

	useEffect(() => {
		void load();
	}, [load, reload]);

	// Which folders were open last time, read once the tree is here so that a
	// project nothing has ever been folded in can open on its sections rather
	// than on five headings with nothing under them. A store that cannot be
	// read leaves the sidebar folded rather than stopping the project opening.
	const restored = useRef<string | null>(null);

	useEffect(() => {
		if (tree.length === 0 || restored.current === root) {
			return;
		}

		restored.current = root;
		invoke<string[]>("read_expanded", { root })
			.then((ids) =>
				setOpen(
					ids.length > 0
						? new Set(ids)
						: new Set(sections(tree).map((each) => each.id)),
				),
			)
			.catch(() => setOpen(new Set()));
	}, [root, tree]);

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

	// Every section folded, or every one open when none of them is. Bound on
	// the window rather than on an editor, the way search is: the sidebar is
	// there whatever has focus, and a writer folding the tree is usually
	// looking at it rather than typing into it.
	const fold = useCallback(async () => {
		const next = folded(open, tree);
		setOpen(next);

		try {
			await invoke("write_expanded", { root, open: [...next] });
		} catch (error) {
			setStatus({ kind: "error", message: failure(error).message });
		}
	}, [open, tree, root]);

	useEffect(() => {
		function onKeyDown(event: KeyboardEvent) {
			if (pressed(event, FOLD)) {
				event.preventDefault();
				void fold();
			}
		}

		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, [fold]);

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
			ref={panel}
			className={hidden ? "sidebar sidebar--hidden" : "sidebar"}
			aria-label="Documents"
			style={{ width }}
		>
			<div className="sidebar__list">
				<div className="sidebar__sections">
					{sections(tree).map((section) => {
						// A section folds through the same set as every folder
						// under it, so one press of the chord reaches all of
						// them and a section remembers itself like the rest.
						const shut = !open.has(section.id);
						const verb = shut ? "Unfold" : "Fold";
						const glyph = shut ? "chevron-right" : "chevron-down";
						const list = shut
							? []
							: rows(section.children, section.id, open);
						// Nowhere inside what is being dragged can be where it
						// lands. Only the section holding it has any such rows;
						// for the others this comes back empty.
						const held =
							reorder.dragging === null
								? NONE
								: inside(list, reorder.dragging);
						// Every row in the list asks for its drag the same way.
						const draggable = (row: Row) =>
							reorder.item(spot(row, section.name, held));

						return (
							<section key={section.id} className="section">
								<div className="section__header">
									{/* In the slot every row below keeps for
									    its own chevron, so the eyebrow starts
									    where their names do. */}
									<button
										type="button"
										className="section__fold"
										aria-expanded={!shut}
										aria-label={`${verb} ${section.name}`}
										onClick={() => void toggle(section.id)}
									>
										<Icon
											name={glyph}
											className="disclosure__glyph"
										/>
									</button>
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
													trail: [section.name],
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
														drag={draggable(row)}
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
																trail: row.trail,
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
													drag={draggable(row)}
													onOpen={() =>
														onSelect(row.document)
													}
													onSubject={() =>
														onSubject(row.document)
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
															row.document.title,
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
									!shut && (
										<p className="section__empty">
											Nothing here yet
										</p>
									)
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

			<Grip
				side="left"
				panel={panel}
				width={width}
				bounds={SIDEBAR}
				label="Sidebar width"
				onWidth={onWidth}
			/>
		</nav>
	);
}
