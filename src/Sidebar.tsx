import {
	useCallback,
	useEffect,
	useLayoutEffect,
	useRef,
	useState,
} from "react";
import type { KeyboardEvent as KeyPress, RefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import BookHead from "./BookHead";
import DocumentRow from "./DocumentRow";
import FolderRow from "./FolderRow";
import Grip from "./Grip";
import Icon from "./Icon";
import {
	MenuItem,
	PointerMenu,
	pointOf,
	useDismissal,
	useRightClick,
	type Point,
} from "./Menu";
import { SIDEBAR } from "./panels";
import NameField from "./NameField";
import NewMenu from "./NewMenu";
import type { FolderNode, ProjectDocument, TreeNode } from "./types";
import type { Making } from "./kinds";
import { MANUSCRIPT, folderKinds, suggested } from "./kinds";
import { useFormat } from "./projectFormat";
import {
	createDocument,
	createFolder,
	moveNode,
	renameDocument,
	renameFolder,
} from "./documents";
import { failure } from "./errors";
import { FOLD, pressed } from "./formatting";
import { useReorder, type Draggable } from "./reorder";
import type { Renaming } from "./rows";
import { indent, retitling, spot } from "./rows";
import { Count } from "./RowProgress";
import { TargetField } from "./Target";
import { MERGE_HOLDS } from "./branches";
import { timed } from "./timing";
import {
	firstScene,
	folded,
	foldersAbove,
	inside,
	pagesBy,
	pruned,
	rows,
	sections,
	treeStep,
	wordsIn,
} from "./tree";
import type { FolderRef, Row } from "./tree";

/** Nothing is being dragged, so no row is inside anything. */
const NONE: ReadonlySet<string> = new Set();

type Status = { kind: "idle" } | { kind: "error"; message: string };

// At most one thing is being named at a time, so where it is going and what it
// is ride along with the state rather than sitting beside it.
type Naming =
	| { kind: "closed" }
	| { kind: "open"; at: string; making: Making; leaving?: boolean }
	| { kind: "creating"; at: string; making: Making }
	| { kind: "refused"; at: string; making: Making; message: string };

/** A binder row on its way to the page. */
export type BinderDrag =
	| { kind: "document"; document: ProjectDocument }
	| { kind: "folder"; folder: FolderRef };

type Props = {
	// Styled out rather than unmounted, so a half-typed section name and the
	// sections already read are still there when it comes back.
	hidden: boolean;
	/** Whether it is open over the writing, to close on a click elsewhere. */
	floating: boolean;
	onDismiss: () => void;
	/** The button in the bar that opens it. */
	opener: RefObject<HTMLButtonElement | null>;
	/** How wide the writer has dragged it, in pixels. */
	width: number;
	onWidth: (width: number) => void;
	root: string;
	/** The project's name, which stands in until the book has a title. */
	name: string;
	// Changes when the project view has altered the manifest.
	reload: number;
	/** Bumped when Book details has written the book. */
	bookWritten: number;
	// The open documents with unwritten changes. The project view already
	// knows which those are; working it out again here would mean a second
	// answer that could disagree with the tab strip.
	unsaved: string[];
	selectedId: string | null;
	/** The id of the folder whose overview is showing, if one is. */
	selectedFolder: string | null;
	selectedTrash: boolean;
	merging: boolean;
	/** What the book menu over Compare or Merge could not do. */
	trouble: { message: string } | null;
	/** Opens a document, beside on the side the writer is not in. */
	onSelect: (document: ProjectDocument, beside: boolean) => void;
	/** Opens the page about a character or a place, for the rows that are one. */
	onSubject: (document: ProjectDocument) => void;
	onTry: (document: ProjectDocument) => Promise<void>;
	onOpenFolder: (folder: FolderRef, beside: boolean) => void;
	/** Opens the timeline of the Manuscript, or of a story in a collection. */
	onTimeline: (folder: FolderRef) => void;
	onOpenTrash: (beside: boolean) => void;
	/** Opens the page about the book the project adds up to. */
	onOpenBook: (beside: boolean) => void;
	onCreated: (document: ProjectDocument) => void;
	/** The renamed document, and the title it answered to before. */
	onRenamed: (document: ProjectDocument, was: string) => void;
	onFolderRenamed: (folder: FolderNode) => void;
	/** Sets or clears (with null) the word target of a document or a folder. */
	onRetarget: (id: string, target: number | null) => Promise<void>;
	onRetargetFolder: (id: string, target: number | null) => Promise<void>;
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
	/**
	 * Each scene's pages in a screenplay, said again every time the tree is
	 * read, for the counts outside the binder. Has to be stable, as above.
	 */
	onPages: (pages: ReadonlyMap<string, number>) => void;
	onDelete: (id: string) => Promise<void>;
	onDeleteFolder: (id: string) => Promise<void>;
	onClose: () => void;
	/** A row picked up, which the page can open where it is dropped. */
	onDrag: (what: BinderDrag | null) => void;
	/** Opens compare at a checkpoint, listing the given version's history. */
	onCompare: (checkpoint: string, slug: string) => void;
	/** Writes every open tab, so asking what is not in a checkpoint sees it. */
	onBeforeSwitch: () => Promise<void>;
	/** Runs a switch the way put back runs, catching the tabs up after it. */
	onSwitch: (task: () => Promise<unknown>) => Promise<boolean>;
	/** Reads the merge again after a switch or a merge from here. */
	onMergeChanged: () => Promise<void>;
	/** Whether rows show their counts and target rings. */
	counts: boolean;
	onCounts: (counts: boolean) => void;
};

export default function Sidebar({
	hidden,
	floating,
	onDismiss,
	opener,
	width,
	onWidth,
	root,
	name,
	reload,
	bookWritten,
	unsaved,
	selectedId,
	selectedFolder,
	selectedTrash,
	merging,
	trouble,
	onSelect,
	onSubject,
	onTry,
	onOpenFolder,
	onTimeline,
	onOpenTrash,
	onOpenBook,
	onCreated,
	onRenamed,
	onFolderRenamed,
	onRetarget,
	onRetargetFolder,
	onChanged,
	onMoved,
	onWords,
	onPages,
	onDelete,
	onDeleteFolder,
	onClose,
	onDrag,
	onCompare,
	onBeforeSwitch,
	onSwitch,
	onMergeChanged,
	counts,
	onCounts,
}: Props) {
	const trash = useRightClick();
	const format = useFormat();
	// The section heading right-clicked, and where. Kept after the menu shuts,
	// so its items still have a section to open while they fade.
	const [sectionAsked, setSectionAsked] = useState<{
		folder: FolderRef;
		at: Point;
		open: boolean;
	} | null>(null);
	// Whether the Manuscript heading's count is a target field.
	const [aimingBook, setAimingBook] = useState(false);
	// Held here rather than passed in, so the element the grip moves is the
	// one this component drew.
	const panel = useRef<HTMLElement>(null);
	useDismissal(floating, panel, onDismiss, opener);
	const [tree, setTree] = useState<TreeNode[]>([]);
	const [status, setStatus] = useState<Status>({ kind: "idle" });
	// A target refused is said in the status line, and the field stays open.
	function aimed(done: Promise<void>) {
		return done.catch((error: unknown) => {
			setStatus({ kind: "error", message: failure(error).message });
			throw error;
		});
	}
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
	// book menu's refresh, which bumps `reload` once it has done so.
	// Counts the reads, so a slow one that finishes after a newer one cannot
	// put the older tree back. Saying nothing while it reads keeps a message
	// from somewhere else from being wiped by a reload. The word count goes
	// out through a ref, so a new function from the parent does not read the
	// manifest again.
	const reads = useRef(0);
	const told = useRef(onWords);
	useEffect(() => {
		told.current = onWords;
	});
	const load = useCallback(async () => {
		reads.current += 1;
		const mine = reads.current;
		try {
			const read = await timed(
				"document tree",
				invoke<TreeNode[]>("document_tree", { root }),
			);
			if (mine !== reads.current) {
				return;
			}
			setTree(read);
			told.current(wordsIn(read));
			onPages(pagesBy(read));
		} catch (error) {
			if (mine === reads.current) {
				setStatus({ kind: "error", message: failure(error).message });
			}
		}
	}, [root, onPages]);

	useEffect(() => {
		void load();
	}, [load, reload]);

	useEffect(() => {
		if (trouble !== null) {
			setStatus({ kind: "error", message: trouble.message });
		}
	}, [trouble]);

	// Which folders were open last time, read once the tree is here so that a
	// project nothing has ever been folded in can open on its sections rather
	// than on five headings with nothing under them, and on the way down to
	// its first scene, which a new project opens on. One the writer shut
	// everything in comes back shut. A store that cannot be read leaves the
	// sidebar folded rather than stopping the project opening.
	const restored = useRef<string | null>(null);

	useEffect(() => {
		if (tree.length === 0 || restored.current === root) {
			return;
		}

		restored.current = root;
		invoke<string[] | null>("read_expanded", { root })
			.then((ids) => {
				const first = firstScene(tree);
				setOpen(
					new Set(
						ids ?? [
							...sections(tree).map((each) => each.id),
							...(first === null
								? []
								: foldersAbove(tree, first.id)),
						],
					),
				);
			})
			.catch(() => setOpen(new Set()));
	}, [root, tree]);

	// Draws these folders open and remembers them for next time.
	const keep = useCallback(
		async (next: ReadonlySet<string>) => {
			setOpen(next);

			try {
				await invoke("write_expanded", { root, open: [...next] });
			} catch (error) {
				setStatus({ kind: "error", message: failure(error).message });
			}
		},
		[root],
	);

	async function toggle(id: string) {
		const next = new Set(open);
		if (!next.delete(id)) {
			next.add(id);
		}
		await keep(pruned(next, tree));
	}

	// Every section folded, or every one open when none of them is. Bound on
	// the window rather than on an editor, the way search is: the sidebar is
	// there whatever has focus, and a writer folding the tree is usually
	// looking at it rather than typing into it.
	const fold = useCallback(
		() => keep(pruned(folded(open, tree), tree)),
		[open, tree, keep],
	);

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

	async function startTake(document: ProjectDocument) {
		try {
			await onTry(document);
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
			// Whatever was made sits inside `at`, so it opens to show it.
			if (!open.has(at)) {
				void keep(new Set(open).add(at));
			}
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

	// The row asking what to call the new thing, at the depth it will land at.
	// Both a section header and a folder inside it want the same one.
	function field(at: string, where: string, depth: number, holds: number) {
		if (naming.kind === "closed") {
			return null;
		}
		const { making } = naming;

		return (
			<li
				className="documents__item"
				style={indent(depth)}
				data-leaving={
					naming.kind === "open" && naming.leaving ? "" : undefined
				}
				onAnimationEnd={(event) => {
					if (event.animationName === "adding-out") {
						setNaming({ kind: "closed" });
					}
				}}
			>
				<div className="sidebar__field adding">
					<span
						className={
							making.kind === "part"
								? "row__icon row__icon--part"
								: "row__icon"
						}
						aria-hidden="true"
					>
						<Icon
							name={making.what === "folder" ? "folder" : "file"}
						/>
					</span>
					<NameField
						label={`Name of the new ${making.noun} in ${where}`}
						placeholder={making.placeholder}
						initial={suggested(making.noun, holds)}
						busy={naming.kind === "creating"}
						error={
							naming.kind === "refused" ? naming.message : null
						}
						onSubmit={(name) => void create(at, making, name)}
						onCancel={(away) =>
							// A click elsewhere lets it fade; Escape is at once.
							setNaming(
								away
									? {
											kind: "open",
											at,
											making,
											leaving: true,
										}
									: { kind: "closed" },
							)
						}
					/>
					<span className="adding__hint" aria-hidden="true">
						<kbd>Enter</kbd> add · <kbd>Esc</kbd>
					</span>
				</div>
			</li>
		);
	}

	// One row in the tab order: the last one that had focus, else the one
	// showing, else the first. The arrow keys move between the rest.
	const binder = useRef<HTMLDivElement>(null);
	const active = useRef<HTMLElement | null>(null);
	const rove = useCallback(() => {
		const rows = [
			...(binder.current?.querySelectorAll<HTMLElement>(".tree__row") ??
				[]),
		];
		const kept =
			rows.find((row) => row === active.current) ??
			rows.find((row) => row.getAttribute("aria-current") === "page") ??
			rows[0];
		for (const row of rows) {
			row.tabIndex = row === kept ? 0 : -1;
		}
	}, []);
	useLayoutEffect(rove);

	function onTreeKey(event: KeyPress<HTMLDivElement>) {
		const row = event.target;
		if (
			!(row instanceof HTMLElement) ||
			!row.classList.contains("tree__row") ||
			event.altKey ||
			event.ctrlKey ||
			event.metaKey ||
			event.shiftKey
		) {
			return;
		}
		const rows = [
			...event.currentTarget.querySelectorAll<HTMLElement>(".tree__row"),
		];
		const fold = row
			.closest("li, .section__header")
			?.querySelector<HTMLButtonElement>(
				":scope > .disclosure, :scope > .section__fold",
			);
		const step = treeStep(
			event.key,
			rows.indexOf(row),
			rows.map((each) => Number(each.dataset.depth)),
			fold ? fold.getAttribute("aria-expanded") === "true" : null,
		);
		if (step === null) {
			return;
		}
		event.preventDefault();
		if (step.kind === "toggle") {
			fold?.click();
		} else {
			rows[step.to].focus();
		}
	}

	return (
		<nav
			ref={panel}
			className={hidden ? "sidebar sidebar--hidden" : "sidebar"}
			aria-label="Documents"
			style={{ width }}
		>
			<BookHead
				root={root}
				name={name}
				reload={reload}
				written={bookWritten}
				merging={merging}
				onOpenBook={() => onOpenBook(false)}
				onBookBeside={() => onOpenBook(true)}
				onClose={onClose}
				onChanged={onChanged}
				onCompare={onCompare}
				onBeforeSwitch={onBeforeSwitch}
				onSwitch={onSwitch}
				onMergeChanged={onMergeChanged}
				counts={counts}
				onCounts={onCounts}
				onMessage={(message) =>
					setStatus(
						message === null
							? { kind: "idle" }
							: { kind: "error", message },
					)
				}
			/>

			<div className="sidebar__list">
				<div
					ref={binder}
					className="sidebar__sections"
					data-counts={counts ? undefined : "off"}
					onKeyDown={onTreeKey}
					onFocus={(event) => {
						if (
							event.target instanceof HTMLElement &&
							event.target.classList.contains("tree__row")
						) {
							active.current = event.target;
							rove();
						}
					}}
				>
					{sections(tree).map((section) => {
						// A section folds through the same set as every folder
						// under it, so one press of the chord reaches all of
						// them and a section remembers itself like the rest.
						const shut = !open.has(section.id);
						const verb = shut ? "Unfold" : "Fold";
						const list = shut
							? []
							: rows(section.children, section.id, open, 0, [
									section.name,
								]);
						// Nowhere inside what is being dragged can be where it
						// lands. Only the section holding it has any such rows;
						// for the others this comes back empty.
						const held =
							reorder.dragging === null
								? NONE
								: inside(list, reorder.dragging);
						// Rust adopts a new file last in its folder, so the
						// field asking its name opens after the last row drawn
						// inside that folder.
						const host = list.find(
							(row) =>
								naming.kind !== "closed" &&
								row.kind === "folder" &&
								row.id === naming.at,
						);
						const landing =
							host?.kind === "folder"
								? ([...inside(list, host.id)].pop() ?? host.id)
								: null;
						const adding =
							host?.kind === "folder" &&
							field(
								host.id,
								host.name,
								host.depth + 1,
								host.holds,
							);
						const atEnd =
							naming.kind !== "closed" &&
							naming.at === section.id &&
							field(
								section.id,
								section.name,
								0,
								section.children.length,
							);
						// Every row in the list asks for its drag the same way.
						// A row can always be dragged to the page, since
						// opening is allowed while a merge waits. Only the
						// reorder drop is refused then, by not answering it.
						const draggable = (row: Row): Draggable => {
							const item = reorder.item(
								spot(row, section.name, held),
							);
							return {
								...item,
								onDragStart: (event) => {
									item.onDragStart(event);
									onDrag(
										row.kind === "document"
											? {
													kind: "document",
													document: row.document,
												}
											: {
													kind: "folder",
													folder: rowRef(
														row,
														section.name,
													),
												},
									);
								},
								onDragEnd: () => {
									item.onDragEnd();
									onDrag(null);
								},
								...(merging && {
									onDragOver: () => {},
									onDrop: () => {},
								}),
							};
						};

						return (
							<section key={section.id} className="section">
								<div className="section__header">
									{/* In the slot every row below keeps for
									    its own chevron, so the eyebrow starts
									    where their names do. */}
									<button
										type="button"
										className="section__fold"
										tabIndex={-1}
										aria-expanded={!shut}
										aria-label={`${verb} ${section.name}`}
										onClick={() => void toggle(section.id)}
									>
										<Icon name="chevron-down" />
									</button>
									<h3 className="section__title">
										<button
											type="button"
											className="section__open tree__row"
											data-depth={-1}
											aria-current={
												section.id === selectedFolder
													? "page"
													: undefined
											}
											onClick={() =>
												onOpenFolder(
													sectionRef(section),
													false,
												)
											}
											onContextMenu={(event) => {
												event.preventDefault();
												setSectionAsked({
													folder: sectionRef(section),
													at: pointOf(event),
													open: true,
												});
											}}
											// To the page only: a section has
											// no place in the binder to move to.
											draggable
											onDragStart={(event) => {
												event.dataTransfer.effectAllowed =
													"move";
												event.dataTransfer.setData(
													"text/plain",
													section.id,
												);
												onDrag({
													kind: "folder",
													folder: sectionRef(section),
												});
											}}
											onDragEnd={() => onDrag(null)}
										>
											{section.name}
										</button>
									</h3>
									{section.name === MANUSCRIPT &&
										(aimingBook ? (
											<TargetField
												compact
												target={section.target}
												pages={section.pages}
												onTarget={(target) =>
													aimed(
														onRetargetFolder(
															section.id,
															target,
														),
													)
												}
												onDone={() =>
													setAimingBook(false)
												}
											/>
										) : (
											<Count
												words={section.words}
												pages={section.pages}
												target={section.target}
											/>
										))}
									<NewMenu
										label={`New in ${section.name}`}
										kind={section.kind}
										section={section.name}
										held={folderKinds(section.children)}
										merging={merging}
										onChoose={(making) =>
											making.name
												? void create(
														section.id,
														making,
														making.name,
													)
												: setNaming({
														kind: "open",
														at: section.id,
														making,
													})
										}
									/>
								</div>

								{list.length > 0 || atEnd ? (
									<ul className="documents">
										{list.map((row) =>
											row.kind === "folder" ? (
												<FolderRow
													key={row.id}
													root={root}
													row={row}
													section={section.name}
													drag={draggable(row)}
													merging={merging}
													current={
														row.id ===
														selectedFolder
													}
													renaming={retitling(
														renaming,
														row.id,
													)}
													after={
														landing === row.id &&
														adding
													}
													onOpen={(beside) =>
														onOpenFolder(
															rowRef(
																row,
																section.name,
															),
															beside,
														)
													}
													onNew={(making) => {
														setNaming({
															kind: "open",
															at: row.id,
															making,
														});
														// The field opens after
														// what the folder holds.
														if (!open.has(row.id)) {
															void keep(
																new Set(
																	open,
																).add(row.id),
															);
														}
													}}
													onMove={(parentId, at) =>
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
													onTarget={(target) =>
														aimed(
															onRetargetFolder(
																row.id,
																target,
															),
														)
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
													onTimeline={() =>
														onTimeline(
															rowRef(
																row,
																section.name,
															),
														)
													}
												/>
											) : (
												<DocumentRow
													key={row.document.id}
													after={
														landing ===
															row.document.id &&
														adding
													}
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
													merging={merging}
													onOpen={(beside) =>
														onSelect(
															row.document,
															beside,
														)
													}
													onSubject={() =>
														onSubject(row.document)
													}
													onTry={() =>
														void startTake(
															row.document,
														)
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
													onTarget={(target) =>
														aimed(
															onRetarget(
																row.document.id,
																target,
															),
														)
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
										{atEnd}
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
					onClick={() => onOpenTrash(false)}
					onContextMenu={trash.open}
				>
					<Icon name="trash" />
					Trash
				</button>
				<PointerMenu
					at={sectionAsked?.open === true ? sectionAsked.at : null}
					label="Section"
					onClose={() =>
						setSectionAsked((now) =>
							now === null ? now : { ...now, open: false },
						)
					}
				>
					{(close) =>
						sectionAsked !== null && (
							<>
								<MenuItem
									onSelect={() => {
										close();
										onOpenFolder(
											sectionAsked.folder,
											false,
										);
									}}
								>
									Open
								</MenuItem>
								<MenuItem
									onSelect={() => {
										close();
										onOpenFolder(sectionAsked.folder, true);
									}}
								>
									Open beside
								</MenuItem>
								{/* A target on the Manuscript is the book's.
								    Research and the rest measure nothing the
								    writer is aiming at. */}
								{/* A collection's timeline is a story's, from
								    the story's own menu. */}
								{sectionAsked.folder.name === MANUSCRIPT &&
									format !== "short-stories" && (
										<MenuItem
											onSelect={() => {
												close();
												onTimeline(sectionAsked.folder);
											}}
										>
											Timeline
										</MenuItem>
									)}
								{sectionAsked.folder.name === MANUSCRIPT && (
									<MenuItem
										disabled={merging}
										title={
											merging ? MERGE_HOLDS : undefined
										}
										onSelect={() => {
											close();
											setAimingBook(true);
										}}
									>
										Set target…
									</MenuItem>
								)}
							</>
						)
					}
				</PointerMenu>
				<PointerMenu at={trash.at} label="Trash" onClose={trash.close}>
					{(close) => (
						<>
							<MenuItem
								onSelect={() => {
									close();
									onOpenTrash(false);
								}}
							>
								Open
							</MenuItem>
							<MenuItem
								onSelect={() => {
									close();
									onOpenTrash(true);
								}}
							>
								Open beside
							</MenuItem>
						</>
					)}
				</PointerMenu>
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

/** A folder row as its overview opens. */
function rowRef(
	row: Extract<Row, { kind: "folder" }>,
	section: string,
): FolderRef {
	return {
		id: row.id,
		name: row.name,
		kind: row.folderKind,
		section,
		trail: row.trail,
	};
}

/** A section as its overview opens: the top of its own trail. */
function sectionRef(section: FolderNode): FolderRef {
	return {
		id: section.id,
		name: section.name,
		kind: section.kind,
		section: section.name,
		trail: [section.name],
	};
}
