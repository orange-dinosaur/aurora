import {
	useEffect,
	useMemo,
	useRef,
	useState,
	type CSSProperties,
	type DragEvent,
	type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { NARROW } from "./breakpoints";
import { filed, refile } from "./corpus";
import Editor from "./Editor";
import { useMedia } from "./useMedia";
import { useFolderFields, useScriptNames, type FieldsHandle } from "./fields";
import { flushAll, flushOthers, holdUnsaved } from "./flushing";
import { list } from "./frontmatter";
import type { About } from "./Mentions";
import FolderView from "./FolderView";
import RightSidebar from "./RightSidebar";
import Sidebar, { type BinderDrag } from "./Sidebar";
import SubjectView from "./SubjectView";
import TagView from "./TagView";
import Tabs from "./Tabs";
import Titlebar from "./Titlebar";
import Trash from "./Trash";
import BookView from "./Book";
import { BookMenu } from "./BookHead";
import Compare from "./Compare";
import Grip from "./Grip";
import Icon from "./Icon";
import { divider, share, SIDE_MIN, standing } from "./panels";
import Overwrite from "./Overwrite";
import Merge from "./Merge";
import { overwritten, type Overwrites } from "./lib/versions";
import type {
	FolderNode,
	Preferences,
	ProjectDocument,
	Status,
	TreeNode,
} from "./types";
import type { FolderRef } from "./tree";
import { documentsOf, firstScene, folderOf, rows } from "./tree";
import { cueNames } from "./fountain";
import type { Opened } from "./bindings/Opened";
import type { Workspace } from "./bindings/Workspace";
import {
	act,
	activate,
	close,
	closeBack,
	dropOn,
	fits,
	focus,
	follow,
	layout,
	noTrail,
	openBeside,
	press,
	prune,
	rekey,
	remember,
	single,
	swap,
	third,
	type Sides as OnPage,
	type Trail,
} from "./split";
import { remembered, reopen, type Place } from "./workspace";
import {
	deleteDocument,
	deleteFolder,
	documentTree,
	restoreDocument,
	writeDocument,
} from "./documents";
import {
	chooseTake,
	deleteTake,
	readTakes,
	renameTake,
	takeText,
	trySomething,
} from "./takes";
import type { Seed } from "./lib/find";
import type { OutlineHandle } from "./lib/outline";
import type { MarksHandle } from "./TakeStrip";
import { ended, gapOf, tick } from "./sessions";
import { useMergeState } from "./useMergeState";
import { useRetagging } from "./useRetagging";
import { useSessions } from "./useSessions";
import { failure } from "./errors";
import {
	repointed,
	strip,
	without,
	type Content,
	type DocumentTab,
	type Save,
	type Tab,
} from "./tabbing";
import { pressed, SEARCH, SESSION } from "./formatting";

type Props = {
	name: string;
	root: string;
	/** A repository the project sits inside, while the writer has not been told. */
	enclosingRepository: string | null;
	/** Made just now, so it opens on its first scene instead of on nothing. */
	justCreated: boolean;
	preferences: Preferences;
	onClose: () => void;
	onSettings: (keyboard: boolean) => void;
	onLogin: () => void;
	onProfile: () => void;
	onPreferences: (change: Partial<Preferences>) => void;
};

/**
 * Where an opened tab goes: the current side, across a split, beside, or the
 * side of the page it was dropped on.
 */
type Landing = "here" | "across" | "beside" | "left" | "right";

/** What is being dragged toward the page. */
type Dragged = { kind: "tab"; key: string } | BinderDrag;

/** How long the writer has to stop typing before the tab is written to disk. */
const AUTOSAVE_MS = 800;

// How long the Control+W chord waits for its second key.
const CHORD_PAUSE_MS = 1500;

async function read(
	root: string,
	id: string,
	seen: Map<string, number>,
): Promise<Content> {
	try {
		const opened = await invoke<Opened>("open_document", { root, id });
		seen.set(id, opened.seen);
		return { kind: "ready", text: opened.text };
	} catch (error) {
		return { kind: "error", message: failure(error).message };
	}
}

export default function Project({
	name,
	root,
	enclosingRepository,
	justCreated,
	preferences,
	onClose,
	onSettings,
	onLogin,
	onProfile,
	onPreferences,
}: Props) {
	const [tabs, setTabs] = useState<Tab[]>([]);

	// False until the tabs the project was left with are back, so the empty
	// strip it opens on is not written over them.
	const [pickedUp, setPickedUp] = useState(false);
	// Where each document's caret and page were left, by id. A ref, because
	// it moves with every keystroke and nothing on screen draws it.
	const places = useRef(new Map<string, Place>());

	// Once only: the dev build runs an effect twice, and a second pass would
	// not yet see the first one's tabs and open them again.
	const openedFirst = useRef(false);
	useEffect(() => {
		if (openedFirst.current) {
			return;
		}
		openedFirst.current = true;

		if (justCreated) {
			setPickedUp(true);
			documentTree(root)
				.then((tree) => {
					const first = firstScene(tree);
					if (first !== null) {
						void openDocument(first);
					}
				})
				// The binder reads the same tree and says what went wrong with it.
				.catch(() => {});
			return;
		}

		Promise.all([
			invoke<Workspace | null>("read_workspace", { root }),
			documentTree(root),
		])
			.then(([saved, tree]) => {
				if (saved === null) {
					return;
				}
				for (const tab of saved.tabs) {
					if (tab.kind === "document") {
						places.current.set(tab.id, {
							caret: tab.caret,
							scroll: tab.scroll,
						});
					}
				}
				const back = reopen(saved, tree);
				const opening = back.tabs.map((tab): Tab =>
					tab.kind === "document"
						? {
								...tab,
								key: freshKey(),
								content: { kind: "loading" },
								save: { kind: "clean" },
								seed: null,
								takes: [],
							}
						: { ...tab, key: freshKey() },
				);
				// Ahead of anything the writer opened while this was reading,
				// less any document they have opened already.
				setTabs((open) => [
					...opening.filter(
						(tab) =>
							tab.kind !== "document" ||
							!open.some(
								(other) =>
									other.kind === "document" &&
									other.document.id === tab.document.id,
							),
					),
					...open,
				]);
				if (back.active !== null) {
					const front = opening[back.active].key;
					const beside = back.beside;
					setOnPage((now) =>
						now.current !== null || now.other !== null
							? now
							: beside === null
								? single(front)
								: {
										current: front,
										other: {
											key: opening[beside.tab].key,
											left: beside.left,
										},
									},
					);
				}
				for (const tab of opening) {
					if (tab.kind === "document") {
						void fill(tab.document.id);
					}
				}
			})
			// Picking up is a convenience: without it the project opens with no
			// tabs, as it always used to.
			.catch(() => {})
			.finally(() => setPickedUp(true));
	}, []);
	// Which tabs are on the page. Without a split, `current` is simply the
	// active tab, which is what most of this view still calls it.
	const [unpruned, setOnPage] = useState<OnPage>(single(null));
	const onPage = prune(unpruned, (key) =>
		tabs.some((tab) => tab.key === key),
	);
	const activeKey = onPage.current;
	function setActiveKey(key: string) {
		setOnPage((now) => activate(now, key));
	}

	// What each side showed before, so closing a side's tab can go back to it.
	const trail = useRef<Trail>(noTrail);
	const shownBefore = useRef(onPage);
	useEffect(() => {
		trail.current = remember(trail.current, shownBefore.current, onPage);
		shownBefore.current = onPage;
	}, [onPage.current, onPage.other?.key, onPage.other?.left]);

	// The width the sides share, which the divider is dragged across.
	const panes = useRef<HTMLDivElement>(null);
	const [room, setRoom] = useState(0);
	useEffect(() => {
		if (panes.current === null) {
			return;
		}
		const watcher = new ResizeObserver(([entry]) =>
			setRoom(entry.contentRect.width),
		);
		watcher.observe(panes.current);
		return () => watcher.disconnect();
	}, []);

	// The tab or binder row being dragged toward the page, and the zone it is
	// over.
	const [dragged, setDragged] = useState<Dragged | null>(null);
	const [zone, setZone] = useState<"left" | "right" | null>(null);
	function drag(what: Dragged | null) {
		setDragged(what);
		setZone(null);
	}

	// The drag leaves pixels on the element. Put a share back, so the sides
	// keep their proportion as the window changes, even when the share comes
	// out the same and React would see nothing to write.
	function divide(splitShare: number) {
		panes.current?.style.setProperty("--split", `${splitShare}%`);
		onPreferences({ splitShare });
	}

	// Bumped when a split key moves the current side, so the editor now in
	// front takes the caret and typing follows the accent line.
	const [summoned, setSummoned] = useState(0);
	const sidesNow = useRef(onPage);
	useEffect(() => {
		sidesNow.current = onPage;
	});
	// Whether Compare or Merge covers the page, set as each render works it out.
	const coveredNow = useRef(false);

	// On the window, ahead of the editor, so the keys work whatever is
	// focused, a folder overview or Search as much as a document.
	useEffect(() => {
		let waiting = false;
		let expiry = 0;
		function onKey(event: KeyboardEvent) {
			// Compare and Merge have no sides to move.
			if (coveredNow.current) {
				return;
			}
			const split = press(waiting, event);
			// The chord gives up after a pause, so a later key types as usual.
			if (split.waiting && !waiting) {
				window.clearTimeout(expiry);
				expiry = window.setTimeout(() => {
					waiting = false;
				}, CHORD_PAUSE_MS);
			}
			waiting = split.waiting;
			if (!split.swallow) {
				return;
			}
			event.preventDefault();
			event.stopPropagation();
			if (split.action === null) {
				return;
			}
			const now = sidesNow.current;
			const after = act(now, split.action);
			setOnPage(after);
			// Typing must not go on landing in the side just left.
			if (after.current !== now.current) {
				(document.activeElement as HTMLElement | null)?.blur();
				setSummoned((times) => times + 1);
			}
		}
		window.addEventListener("keydown", onKey, true);
		return () => {
			window.clearTimeout(expiry);
			window.removeEventListener("keydown", onKey, true);
		};
	}, []);

	// Written whenever the strip changes shape, not on every keystroke that
	// passes through the tabs. Carets and scrolls ride along with whatever
	// they were last, and on the way out.
	const shape = JSON.stringify(
		remembered(tabs, activeKey, new Map(), onPage.other),
	);
	const leftWith = useRef<(() => Workspace) | null>(null);
	useEffect(() => {
		if (!pickedUp) {
			return;
		}
		leftWith.current = () =>
			remembered(tabs, activeKey, places.current, onPage.other);
		void keepWorkspace();
	}, [pickedUp, shape]);

	// Null until the tabs are back, so quitting first cannot write an empty
	// strip over them.
	async function keepWorkspace() {
		if (leftWith.current === null) {
			return;
		}
		await invoke("write_workspace", {
			root,
			workspace: leftWith.current(),
		}).catch(() => {});
	}
	// Bumped whenever this view changes what the manifest holds, so the sidebar
	// knows to read it again.
	const [listing, setListing] = useState(0);
	// Bumped every time search is asked for, so asking again while its panel
	// is already open reaches the field rather than doing nothing.
	const [asked, setAsked] = useState(0);
	// Bumped whenever a document reaches disk, which is the other way the
	// project changes under anything holding a copy of it.
	const [written, setWritten] = useState(0);
	const cast = useScriptNames(root, listing + written);

	// A document created, renamed, moved or deleted. Every other change is a
	// text the corpus has already been handed, but this one it cannot work out
	// for itself.
	useEffect(() => {
		refile(root);
	}, [root, listing]);
	// The open document's front matter, handed up by whichever editor is
	// showing. The panel that displays it is rendered out here, beside the
	// editor rather than inside it, so it cannot read the editor for itself.
	const [fields, setFields] = useState<FieldsHandle | null>(null);
	// The headings of the same document, and for the same reason.
	const [outline, setOutline] = useState<OutlineHandle | null>(null);
	const [marks, setMarks] = useState<MarksHandle | null>(null);
	const active = tabs.find((tab) => tab.key === activeKey) ?? null;

	// A folder's fields are read on their own rather than carried on the tab:
	// the manifest is the only copy, and `listing` says when it has moved.
	const { handle: folderFields, trouble } = useFolderFields(
		root,
		active?.kind === "folder" ? active.folder.id : null,
		listing,
	);

	// A tab's key is its own, handed out when it opens and never derived from
	// what it holds: a rename changes a document's path and a restore can
	// change its id, and neither of those is the tab being replaced.
	const keys = useRef(0);
	function freshKey() {
		keys.current += 1;
		return String(keys.current);
	}

	// One timer per open document, so a tab keeps its own countdown once the
	// writer has moved on to another one.
	const timers = useRef(new Map<string, number>());

	// The modified time of each open document's file as this page last saw it,
	// which a save hands back so the file can refuse to be written over
	// something it has not seen.
	const seen = useRef(new Map<string, number>());

	// This view's own flush as `flushing` holds it, so writing the other
	// screens down can leave it out.
	const ownFlush = useRef<(() => Promise<void>) | null>(null);

	// Remarks typed on a subject page whose document has no tab, by id, until
	// they are on disk.
	const loose = useRef(new Map<string, string>());

	// The write of each document still under way, which the next one waits for.
	const chain = useRef(new Map<string, Promise<unknown>>());

	// The silence that ends an automatic session, as the writer set it. Every
	// call into the model is handed this rather than letting it fall back to
	// its own constant.
	const gap = gapOf(preferences.idleMinutes);
	const {
		sessions,
		words,
		ahead,
		idle,
		deliberate,
		logged,
		unrecordedCount,
		measured,
		keep,
		record,
		startSession,
		stopSession,
		tickSession,
	} = useSessions(root, gap);

	// Bumped when Book details has written the book, which the binder leads with.
	const [bookWritten, setBookWritten] = useState(0);

	// In a narrow window the sidebar floats over the writing until the next
	// click, so it has its own open and the writer's lasting choice is left for
	// the wider window. Watched rather than read once, so resizing across the
	// line swaps one for the other.
	const narrow = useMedia(NARROW);
	const [floating, setFloating] = useState(false);
	const sidebarButton = useRef<HTMLButtonElement>(null);
	const sidebar = narrow ? floating : preferences.sidebar;
	// Compare's and Merge's lists float the same way, from a button in the bar.
	const [listOpen, setListOpen] = useState(false);
	const listButton = useRef<HTMLButtonElement>(null);

	useEffect(() => {
		setFloating(false);
		setListOpen(false);
	}, [narrow]);

	const {
		pieces,
		piecesFailed,
		comparing,
		setComparing,
		merge,
		settling,
		setSettling,
		choices,
		setChoices,
		sides,
		setSides,
		readMerge,
	} = useMergeState(root);
	// The put back question while it waits for an answer.
	const [asking, setAsking] = useState<{
		names: string[];
		answer: (go: boolean) => void;
	} | null>(null);
	// What the book menu over Compare or Merge could not do, said in the
	// binder once the writer is back there. A fresh object each time, so the
	// same failure twice is said twice.
	const [bookTrouble, setBookTrouble] = useState<{ message: string } | null>(
		null,
	);

	// A floating list closes on any way in or out, and on a pick in Merge.
	useEffect(() => setListOpen(false), [comparing, settling]);

	// What a merge needs next is in the Versions panel, so it opens there. Once,
	// so the writer can still close it.
	const underway = merge !== null;
	useEffect(() => {
		if (underway) {
			onPreferences({
				rightSidebar: true,
				rightSidebarTab: "versions",
				versionsScope: "changes",
			});
		}
	}, [underway]);

	// Why a folder's target was refused, when one was.
	const [aiming, setAiming] = useState<string | null>(null);

	// The tabs as they stand now. The handlers below are registered once and
	// would otherwise go on seeing what they were born with.
	const latest = useRef(tabs);
	useEffect(() => {
		latest.current = tabs;
	});

	// Through `latest`, since callers often ask after an await.
	function documentTab(id: string): DocumentTab | undefined {
		return latest.current.find(
			(tab): tab is DocumentTab =>
				tab.kind === "document" && tab.document.id === id,
		);
	}

	// Puts a tab's text on disk, putting the file itself back if it has gone.
	function put(tab: DocumentTab, text: string) {
		const done =
			tab.save.kind === "missing"
				? restoreDocument(root, tab.document.path, text).then(() =>
						seen.current.delete(tab.document.id),
					)
				: save(tab.document.id, text, false);

		return done.then(() => filed(root, tab.document.id, text));
	}

	// Writes a document and keeps the time its file now has. When `checked`,
	// the file refuses if something else has written it since this page last
	// saw it. Each write of a document waits for the one before, so they land
	// in order and each checks against the time the last one left.
	async function save(id: string, text: string, checked: boolean) {
		const before = chain.current.get(id) ?? Promise.resolve();
		const mine = before
			.catch(() => undefined)
			.then(async () => {
				const now = await invoke<number>("save_document", {
					root,
					id,
					text,
					seen: checked ? (seen.current.get(id) ?? null) : null,
				});
				seen.current.set(id, now);
				// ponytail: typing while this write is out is forgotten here too,
				// until the next save; the sidebar's count is off by that much.
				ahead.current.delete(id);
			});
		chain.current.set(id, mine);
		try {
			await mine;
		} finally {
			if (chain.current.get(id) === mine) {
				chain.current.delete(id);
			}
		}
	}

	// Every tab whose text is not on disk yet, with that text.
	function unwritten(): [DocumentTab, string][] {
		return latest.current.flatMap((tab): [DocumentTab, string][] =>
			tab.kind === "document" &&
			tab.save.kind !== "clean" &&
			tab.content.kind === "ready"
				? [[tab, tab.content.text]]
				: [],
		);
	}

	// Writes every tab that is not on disk yet, cancelling the timers that were
	// going to do it, and marks each one clean or failed. Remarks typed with
	// no tab open go down too. Hands back the names of what could not be
	// written.
	async function writeAll(): Promise<string[]> {
		timers.current.forEach(window.clearTimeout);
		timers.current.clear();

		const remarks = await Promise.all(
			[...loose.current].map(([id, text]) =>
				writeLoose(id, text).then(
					() => [],
					() => ["a subject's remarks"],
				),
			),
		);
		const results = await Promise.all(
			unwritten().map(async ([tab, text]) => {
				const id = tab.document.id;
				try {
					await put(tab, text);
				} catch (error) {
					const { message } = failure(error);
					patch(id, (open) => ({
						...open,
						save: { kind: "failed", message },
					}));
					return [tab.document.title];
				}
				patch(id, (open) =>
					open.content.kind === "ready" && open.content.text === text
						? { ...open, save: { kind: "clean" } }
						: open,
				);
				return [];
			}),
		);
		return [...remarks, ...results].flat();
	}

	// Before anything that reads the project from disk: a document that could
	// not be written stops it, since its newest text would not be there. Other
	// screens holding edits, such as Book details, write theirs first.
	async function allWritten() {
		await flushOthers(ownFlush.current);
		const failed = await writeAll();
		if (failed.length > 0) {
			throw new Error(`Could not save ${failed.join(", ")}`);
		}
	}

	async function flush() {
		// Quitting has nowhere left to report a failure.
		await writeAll();

		// A session that had already gone quiet closed as usual. The one the
		// writer is in the middle of is worth as much as the text they were
		// typing into it, and leaving ends it.
		const now = Date.now();
		await keep(tick(sessions.current, now, words.current, idle.current));
		await keep(ended(sessions.current, now, words.current, idle.current));
		await keepWorkspace();
	}

	// Quitting must not lose what the debounce has not written yet. Tauri waits
	// for this handler before it closes the window, so awaiting the writes here
	// is what holds the door. It flushes every screen rather than this one,
	// because the Book screen holds its own unwritten edits.
	useEffect(() => {
		const stopping = getCurrentWindow().onCloseRequested(() => flushAll());
		return () => {
			void stopping.then((stop) => stop());
		};
	}, [root]);

	// Closing the project unmounts this view, and loses the same edits.
	useEffect(() => {
		ownFlush.current = flush;
		const letGo = holdUnsaved(flush);
		return () => {
			letGo();
			ownFlush.current = null;
			void flush();
		};
	}, [root]);

	function stopTimer(id: string) {
		const timer = timers.current.get(id);
		if (timer !== undefined) {
			window.clearTimeout(timer);
			timers.current.delete(id);
		}
	}

	function patch(id: string, change: (tab: DocumentTab) => DocumentTab) {
		setTabs((open) =>
			open.map((tab) =>
				tab.kind === "document" && tab.document.id === id
					? change(tab)
					: tab,
			),
		);
	}

	// Puts a tab on the page. A link followed from inside a side goes `across`,
	// to the other side of a split; without a split it lands `here`. Open
	// beside makes the split if there is none.
	function show(key: string, landing: Landing) {
		setOnPage((now) => {
			switch (landing) {
				case "beside":
					return openBeside(now, key);
				case "across":
					return follow(now, key);
				case "left":
				case "right":
					return dropOn(now, key, landing);
				default:
					return activate(now, key);
			}
		});
	}

	async function openDocument(
		document: ProjectDocument,
		seed: Seed | null = null,
		landing: Landing = "here",
	) {
		const already = documentTab(document.id);
		if (already !== undefined) {
			show(already.key, landing);
			// The tab was already here, so the seed is the only news: the
			// editor is mounted and Find answers it where it stands.
			if (seed !== null) {
				patch(document.id, (tab) => ({ ...tab, seed }));
			}
			return;
		}

		const opening: DocumentTab = {
			kind: "document",
			key: freshKey(),
			document,
			content: { kind: "loading" },
			save: { kind: "clean" },
			seed,
			takes: [],
		};
		setTabs((open) => [...open, opening]);
		show(opening.key, landing);
		await fill(document.id);
	}

	async function fill(id: string) {
		const [content, takes] = await Promise.all([
			read(root, id, seen.current),
			readTakes(root, id),
		]);
		// Keyed by id, so a slow read can only ever fill in its own tab — and
		// quietly does nothing if that tab was closed while it was in flight.
		patch(id, (tab) => ({ ...tab, content, takes }));
	}

	// The writer sees the new take, so a document that was shut opens.
	async function startTake(
		document: ProjectDocument,
		landing: Landing = "here",
	) {
		await putBack(() => trySomething(root, document.id));
		await openDocument(document, null, landing);
	}

	// Raises the tab `match` finds, or opens the one `make` builds.
	function raise(
		match: (tab: Tab) => boolean,
		make: (key: string) => Tab,
		landing: Landing,
	) {
		const already = latest.current.find(match);
		if (already !== undefined) {
			show(already.key, landing);
			return;
		}

		const opening = make(freshKey());
		setTabs((open) => [...open, opening]);
		show(opening.key, landing);
	}

	function openFolder(folder: FolderRef, landing: Landing = "here") {
		raise(
			(tab) => tab.kind === "folder" && tab.folder.id === folder.id,
			(key) => ({ kind: "folder", key, folder }),
			landing,
		);
	}

	function openBook(landing: Landing = "here") {
		raise(
			(tab) => tab.kind === "book",
			(key) => ({ kind: "book", key }),
			landing,
		);
	}

	function openTrash(landing: Landing = "here") {
		raise(
			(tab) => tab.kind === "trash",
			(key) => ({ kind: "trash", key }),
			landing,
		);
	}

	/**
	 * Where a tag chip goes. A tag that is also the title of a document is a
	 * link to it; anything else opens the list of what wears it. The match is
	 * case-sensitive, like recognition in prose, so `elena` is a keyword even
	 * while `Elena` is a page — one rule for both rather than two answers to
	 * the same question.
	 *
	 * The title is looked up as the chip is clicked rather than held anywhere,
	 * so deleting Elena's document turns the link into a list on its own.
	 */
	async function openTag(tag: string, landing: Landing = "here") {
		try {
			const tree = await documentTree(root);
			const named = documentsOf(tree).find(
				(document) => document.title === tag,
			);
			if (named !== undefined) {
				await openDocument(named, null, landing);
				return;
			}
		} catch {
			// The tree could not be read, so nothing can be said to be named
			// this. The list is the honest answer, and reports its own trouble.
		}

		raise(
			(tab) => tab.kind === "tag" && tab.tag === tag,
			(key) => ({ kind: "tag", key, tag }),
			landing,
		);
	}

	/**
	 * The page about a character or a place. It opens as a tab of its own
	 * rather than in place of the document: the page is a reading of the same
	 * file, and the writer should be able to keep both.
	 */
	function openSubject(subject: ProjectDocument, landing: Landing = "here") {
		raise(
			(tab) => tab.kind === "subject" && tab.subject.id === subject.id,
			(key) => ({ kind: "subject", key, subject }),
			landing,
		);
	}

	/**
	 * The page of the character a script's cue names, opened across from the
	 * script. A cue naming no one opens nothing.
	 */
	async function openCue(cue: string) {
		const tree = await documentTree(root).catch(() => []);
		const found = documentsOf(tree).find(
			(document) =>
				document.trail[0] === "Characters" &&
				cueNames(cue, document.title),
		);
		if (found !== undefined) {
			openSubject(found, "beside");
		}
	}

	/**
	 * Moving from one subject's page to another's, by turning the tab rather
	 * than opening a second one. Following a relationship is reading, and a
	 * chain of them would otherwise leave a tab behind at every step. A page
	 * that is already open somewhere else is raised instead.
	 */
	function turnSubject(key: string, subject: ProjectDocument) {
		const already = tabs.find(
			(tab) => tab.kind === "subject" && tab.subject.id === subject.id,
		);
		if (already !== undefined) {
			setActiveKey(already.key);
			return;
		}

		setTabs((open) =>
			open.map((tab) =>
				tab.key === key && tab.kind === "subject"
					? { ...tab, subject }
					: tab,
			),
		);
	}

	function openSearch() {
		onPreferences({
			rightSidebar: true,
			rightSidebarTab: "search",
		});
		// Asking a second time is asking for the field again.
		setAsked((times) => times + 1);
	}

	// Bound on the window, unlike every other shortcut in Aurora. The ones in
	// `Shortcuts` are registered on the editor and so are silent whenever no
	// editor has focus — which is the trash, the overviews, and search itself.
	useEffect(() => {
		function onKeyDown(event: KeyboardEvent) {
			if (pressed(event, SEARCH) && !coveredNow.current) {
				event.preventDefault();
				openSearch();
			}
			// One key both ways round, so a writer who cannot see the titlebar
			// control can still end what they started. The layer is read from
			// the ref, which is current wherever this was registered.
			if (pressed(event, SESSION)) {
				event.preventDefault();
				if (sessions.current.deliberate === null) {
					startSession();
				} else {
					stopSession();
				}
			}
		}

		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, [root]);

	// A new document is opened for writing in, and every listing of it has to
	// be read again.
	// A new document is given the writer's default target, if they set one. It
	// is applied here rather than by whichever surface made the document, so
	// the sidebar and a section's overview cannot disagree about it, and after
	// the document is open, because `retarget` reports a refusal in the panel.
	async function created(
		document: ProjectDocument,
		landing: Landing = "here",
	) {
		await openDocument(document, null, landing);
		if (preferences.defaultTarget !== null) {
			await retarget(document.id, preferences.defaultTarget).catch(
				() => undefined,
			);
		}
		setListing((version) => version + 1);
	}

	// Reordering, and making a folder, change the manifest and nothing else,
	// so every listing of it is told to read it again and nothing is opened.
	function changed() {
		setListing((version) => version + 1);
	}

	// What a rename cannot decide on its own: what to do with the tags that
	// were pointing at the old title.
	const { retagging, countTags, retagAll, dismiss } = useRetagging(root, {
		documentTab,
		stopTimer,
		save,
		patch,
		freshKey,
		setOnPage,
		written: () => setWritten((times) => times + 1),
	});

	// A rename gives a document a new path and title but not a new id, so a
	// tab holding it is re-pointed where it stands. Its key is its own, so
	// neither the strip nor the editor is torn down for this.
	function renamed(document: ProjectDocument, was: string) {
		patch(document.id, (tab) => ({ ...tab, document }));
		setListing((version) => version + 1);

		if (was !== "" && was !== document.title) {
			void countTags(was, document.title);
		}
	}

	// A move changes the path of everything under what moved, which can be a
	// whole chapter of open scenes. Rather than work out which, the tabs are
	// re-pointed from the tree: ids are what a tab holds on to, and a move
	// changes none of them.
	async function moved() {
		setListing((version) => version + 1);
		try {
			repoint(await documentTree(root));
		} catch {
			// The listing above reads the same manifest and shows whatever went
			// wrong with it, so there is nothing to say twice.
		}
	}

	// Points every tab at what the tree holds now, closing the ones whose
	// document or folder has gone.
	function repoint(tree: TreeNode[]) {
		const kept = new Set(
			repointed(latest.current, tree).map((tab) => tab.key),
		);
		dropTabs(
			latest.current.flatMap((tab) =>
				kept.has(tab.key) ? [] : [tab.key],
			),
		);
		const next = repointed(latest.current, tree);
		latest.current = next;
		setTabs(next);
	}

	// Putting back rewrites files behind every tab, so everything open is written
	// down first and brought up to the disk after. A document gone from the
	// project closes, one whose text changed is mounted again from its file,
	// and the rest are re-pointed the way a move re-points them. A failure is
	// thrown back to the surface that asked, after the tabs have caught up with
	// whatever part of it did happen.
	// With a target, asks first when the put back would write over changes
	// not in a checkpoint. A switch passes none. False when the writer declined.
	async function putBack(
		task: () => Promise<unknown>,
		target?: Overwrites,
	): Promise<boolean> {
		await allWritten();
		if (target !== undefined) {
			const names = overwritten(
				await invoke<Status>("version_status", { root }),
				target,
			);
			if (names.length > 0) {
				const go = await new Promise<boolean>((answer) =>
					setAsking({ names, answer }),
				);
				setAsking(null);
				if (!go) {
					return false;
				}
			}
		}
		try {
			await task();
		} finally {
			await caughtUp();
		}
		return true;
	}

	async function caughtUp() {
		setListing((version) => version + 1);
		let tree: TreeNode[];
		try {
			tree = await documentTree(root);
		} catch {
			// The listing reads the same manifest and says what went wrong.
			return;
		}
		const now = new Map(documentsOf(tree).map((doc) => [doc.id, doc]));
		const contents = new Map(
			await Promise.all(
				latest.current.flatMap((tab) =>
					tab.kind === "document" && now.has(tab.document.id)
						? [
								Promise.all([
									read(root, tab.document.id, seen.current),
									readTakes(root, tab.document.id),
								]).then(
									([content, takes]) =>
										[
											tab.document.id,
											{ content, takes },
										] as const,
								),
							]
						: [],
				),
			),
		);

		// Old keys to new, for the tabs mounted again.
		const renamed = new Map<string, string>();
		const again = (tab: Tab) => {
			const key = freshKey();
			renamed.set(tab.key, key);
			return key;
		};
		const next = repointed(latest.current, tree).flatMap((tab): Tab[] => {
			switch (tab.kind) {
				case "book":
					return [{ ...tab, key: again(tab) }];
				case "document": {
					const document = now.get(tab.document.id);
					const found = contents.get(tab.document.id);
					if (document === undefined) {
						return [];
					}
					if (found?.content.kind !== "ready") {
						return [{ ...tab, document }];
					}
					const { content, takes } = found;
					if (
						tab.content.kind === "ready" &&
						tab.content.text === content.text
					) {
						return [
							{
								...tab,
								document,
								takes,
								save: { kind: "clean" },
							},
						];
					}
					return [
						{
							...tab,
							key: again(tab),
							document,
							content,
							takes,
							save: { kind: "clean" },
						},
					];
				}
				default:
					return [tab];
			}
		});
		// A keystroke while this read would otherwise write old text over the
		// file just put back.
		timers.current.forEach(window.clearTimeout);
		timers.current.clear();
		setTabs(next);
		setOnPage((now) => {
			const kept = prune(rekey(now, renamed), (key) =>
				next.some((tab) => tab.key === key),
			);
			const lost =
				now.current !== null &&
				kept.current === null &&
				kept.other === null;
			return lost ? single(next[0]?.key ?? null) : kept;
		});
	}

	// A renamed folder keeps its id, but every path under it changes, so its
	// overview and the tabs of its documents are all re-pointed.
	function renamedFolder(_folder: FolderNode) {
		void moved();
	}

	// A target is the writer's intention rather than their text, so nothing is
	// saved but the manifest — and every listing of the document has to read it
	// again to show the new one on its card.
	// A refusal is said in the panel's line and thrown back to the field, which
	// stays open; the surface that asked says it in its own line too.
	async function retarget(id: string, target: number | null) {
		try {
			const document = await invoke<ProjectDocument>(
				"set_document_target",
				{ root, id, target },
			);
			setAiming(null);
			patch(id, (tab) => ({ ...tab, document }));
			setListing((version) => version + 1);
		} catch (error) {
			setAiming(failure(error).message);
			throw error;
		}
	}

	async function retargetFolder(id: string, target: number | null) {
		try {
			await invoke("set_folder_target", { root, id, target });
			setAiming(null);
			setListing((version) => version + 1);
		} catch (error) {
			setAiming(failure(error).message);
			throw error;
		}
	}

	// A refusal belongs to what was open when it happened.
	useEffect(() => setAiming(null), [activeKey]);

	function edit(id: string, text: string) {
		patch(id, (tab) =>
			tab.content.kind === "ready"
				? {
						...tab,
						content: { kind: "ready", text },
						save:
							tab.save.kind === "changed"
								? tab.save
								: { kind: "pending" },
					}
				: tab,
		);

		stopTimer(id);
		// Nothing is written over a file that changed until the writer chooses.
		if (documentTab(id)?.save.kind === "changed") {
			return;
		}
		timers.current.set(
			id,
			window.setTimeout(() => {
				timers.current.delete(id);
				void write(id, text);
			}, AUTOSAVE_MS),
		);
	}

	// The subject page setting its own document's remarks. A tab is the holder
	// of a file whenever there is one, so this goes through the tab and the
	// countdown the editor already uses, and rebuilds that tab's editor from
	// the new text the way retagging does: an editor still holding front matter
	// that has moved on would put it back the next time it saved. With no tab,
	// the page writes the file itself, on the same countdown.
	function remark(
		id: string,
		text: string,
		failed: (message: string) => void,
	) {
		const tab = documentTab(id);
		if (tab !== undefined) {
			edit(id, text);
			// The side showing the tab follows it to its new key.
			const key = freshKey();
			patch(id, (open) => ({ ...open, key }));
			setOnPage((now) => rekey(now, new Map([[tab.key, key]])));
			return;
		}

		stopTimer(id);
		loose.current.set(id, text);
		timers.current.set(
			id,
			window.setTimeout(() => {
				timers.current.delete(id);
				void writeLoose(id, text).catch((error: unknown) =>
					failed(failure(error).message),
				);
			}, AUTOSAVE_MS),
		);
	}

	// Counted like any other save, so the page reads back what it wrote and
	// lets go of the text it was holding.
	async function writeLoose(id: string, text: string) {
		await writeDocument(root, id, text);
		if (loose.current.get(id) === text) {
			loose.current.delete(id);
		}
		filed(root, id, text);
		setWritten((times) => times + 1);
	}

	async function write(id: string, text: string) {
		patch(id, (tab) => ({ ...tab, save: { kind: "saving" } }));

		let result: Save;
		try {
			await save(id, text, true);
			filed(root, id, text);
			result = { kind: "clean" };
			setWritten((times) => times + 1);
		} catch (error) {
			const { kind, message } = failure(error);
			// The file going missing or changing are the failures the writer
			// can do something about, so each gets its own state rather than a
			// message.
			result =
				kind === "documentMissing"
					? { kind: "missing" }
					: kind === "changedOnDisk"
						? { kind: "changed" }
						: { kind: "failed", message };
		}

		// Only the write that put down what the tab still holds may report on
		// it. If the writer has typed since, a later write is already on its
		// way and this one's verdict is out of date.
		patch(id, (tab) =>
			tab.content.kind === "ready" && tab.content.text === text
				? { ...tab, save: result }
				: tab,
		);
	}

	// Takes the file's text over the tab's. The editor seeds itself from its
	// text once, so the tab comes back under a fresh key, as it does after
	// retagging.
	async function reload(id: string) {
		const tab = documentTab(id);
		if (tab === undefined) {
			return;
		}

		stopTimer(id);
		const content = await read(root, id, seen.current);
		if (content.kind !== "ready") {
			patch(id, (open) => ({ ...open, content }));
			return;
		}

		const key = freshKey();
		patch(id, (open) => ({
			...open,
			key,
			content,
			save: { kind: "clean" },
		}));
		setOnPage((now) => rekey(now, new Map([[tab.key, key]])));
		filed(root, id, content.text);
	}

	// Writes the tab's text over the file's, which the writer has seen and
	// chosen to replace.
	async function keepMine(id: string) {
		const tab = documentTab(id);
		if (tab === undefined || tab.content.kind !== "ready") {
			return;
		}

		const text = tab.content.text;
		patch(id, (open) => ({ ...open, save: { kind: "saving" } }));
		try {
			await save(id, text, false);
			filed(root, id, text);
			setWritten((times) => times + 1);
			patch(id, (open) =>
				open.content.kind === "ready" && open.content.text === text
					? { ...open, save: { kind: "clean" } }
					: { ...open, save: { kind: "pending" } },
			);
		} catch (error) {
			patch(id, (open) => ({
				...open,
				save: { kind: "failed", message: failure(error).message },
			}));
		}
	}

	async function restore(id: string) {
		const tab = documentTab(id);
		if (tab === undefined || tab.content.kind !== "ready") {
			return;
		}

		const text = tab.content.text;
		stopTimer(id);
		patch(id, (open) => ({ ...open, save: { kind: "saving" } }));

		let restored: ProjectDocument;
		try {
			restored = await restoreDocument(root, tab.document.path, text);
		} catch (error) {
			patch(id, (open) => ({
				...open,
				save: { kind: "failed", message: failure(error).message },
			}));
			return;
		}

		// The file is new, so the time the tab held belongs to the one that went.
		seen.current.delete(id);
		// A refresh while the file was away drops the document, so it can come
		// back under a new id and the tab has to follow it.
		patch(id, (open) => ({
			...open,
			document: restored,
			save:
				open.content.kind === "ready" && open.content.text === text
					? { kind: "clean" }
					: { kind: "pending" },
		}));
		setListing((version) => version + 1);
	}

	// Takes a tab out of the strip and moves off it if it was the one being
	// looked at. Reads the tabs through `latest`, since a caller may have
	// awaited something before getting here.
	function dropTab(key: string) {
		dropTabs([key]);
	}

	// Several at once, in one change to the strip, since each change starts
	// from `latest` and a second would undo the first.
	function dropTabs(keys: string[]) {
		const open = latest.current;
		const gone = new Set(
			keys.filter((key) => open.some((tab) => tab.key === key)),
		);
		if (gone.size === 0) {
			return;
		}

		const remaining = without(open, gone);
		latest.current = remaining;
		setTabs(remaining);
		setOnPage((now) => {
			let page = now;
			for (const key of gone) {
				// In a split, the side goes back to what it showed before, or
				// the other side fills the page.
				const after = closeBack(page, key, trail.current, (open) =>
					remaining.some((tab) => tab.key === open),
				);
				const showing =
					after.other !== null ||
					(after.current !== null && after.current !== key);
				const wasShown =
					page.current === key || page.other?.key === key;
				// The nearest one left to its left, or the new first.
				const index = open.findIndex((tab) => tab.key === key);
				const left = open
					.slice(0, index)
					.reverse()
					.find((tab) => !gone.has(tab.key));
				page =
					showing || !wasShown
						? after
						: single((left ?? remaining[0])?.key ?? null);
			}
			return page;
		});
	}

	// Closing must not throw away text that is not on disk yet, so the tab
	// stays open, saying why, when its write fails.
	async function closeTab(key: string) {
		const closing = latest.current.find((tab) => tab.key === key);
		if (closing === undefined) {
			return;
		}

		if (closing.kind === "document") {
			const id = closing.document.id;
			stopTimer(id);
			if (
				closing.save.kind !== "clean" &&
				closing.content.kind === "ready"
			) {
				try {
					await put(closing, closing.content.text);
				} catch (error) {
					const { message } = failure(error);
					patch(id, (tab) => ({
						...tab,
						save: { kind: "failed", message },
					}));
					return;
				}
			}
		}

		dropTab(key);
	}

	// Deleting belongs here rather than on the surface that asked for it,
	// because what the writer last typed is here. The pending write goes down
	// first so the copy in the trash is the one they were looking at, then the
	// file moves, then the tab goes. A failure is thrown back to the surface,
	// which has somewhere to show it.
	//
	// This is the first part: it writes a document's tab down before its file
	// is taken away, and hands back the tab so the caller can close it once
	// the file has gone.
	async function settle(id: string) {
		const tab = documentTab(id);
		stopTimer(id);

		if (
			tab !== undefined &&
			tab.content.kind === "ready" &&
			tab.save.kind !== "clean" &&
			// A tab whose file has gone has nothing to flush to, and putting
			// the file back only to trash it would be absurd.
			tab.save.kind !== "missing" &&
			// Nor does one whose file the writer has not yet chosen to
			// overwrite.
			tab.save.kind !== "changed"
		) {
			await save(id, tab.content.text, true);
			filed(root, id, tab.content.text);
		}

		return tab;
	}

	// The delete that followed `settle` failed, so its tabs stay open and say so.
	function unsettled(ids: string[], error: unknown) {
		const { message } = failure(error);
		for (const id of ids) {
			patch(id, (tab) => ({ ...tab, save: { kind: "failed", message } }));
		}
	}

	async function remove(id: string) {
		const tab = await settle(id);
		try {
			await deleteDocument(root, id);
		} catch (error) {
			unsettled([id], error);
			throw error;
		}

		if (tab !== undefined) {
			dropTab(tab.key);
		}
		// Its subject page goes with it.
		await moved();
	}

	// A folder goes into the trash whole, so everything open inside it is
	// written down first and closed afterwards: its own overview, the overviews
	// of the folders under it, and every scene in any of them.
	async function removeFolder(id: string) {
		const tree = await documentTree(root);
		const folder = folderOf(tree, id);
		const inside = folder === null ? [] : rows(folder.children, id);

		const open = await Promise.all(
			inside.flatMap((row) =>
				row.kind === "document" ? [settle(row.document.id)] : [],
			),
		);
		try {
			await deleteFolder(root, id);
		} catch (error) {
			unsettled(
				inside.flatMap((row) =>
					row.kind === "document" ? [row.document.id] : [],
				),
				error,
			);
			throw error;
		}

		const gone = new Set([
			id,
			...inside.flatMap((row) => (row.kind === "folder" ? [row.id] : [])),
		]);
		dropTabs([
			...latest.current.flatMap((tab) =>
				tab.kind === "folder" && gone.has(tab.folder.id)
					? [tab.key]
					: [],
			),
			...open.flatMap((tab) => (tab === undefined ? [] : [tab.key])),
		]);
		await moved();
	}

	// What every open document says right now, which is ahead of its file for
	// the 800 ms after a keystroke and ahead of anything search read earlier.
	const live = useMemo(
		() =>
			new Map(
				tabs.flatMap((tab) =>
					tab.kind === "document" && tab.content.kind === "ready"
						? [[tab.document.id, tab.content.text] as const]
						: [],
				),
			),
		[tabs],
	);

	function documentBody(tab: DocumentTab) {
		if (tab.content.kind === "ready") {
			return (
				<Editor
					title={tab.document.title}
					trail={tab.document.trail}
					text={tab.content.text}
					active={tab.key === activeKey}
					summoned={summoned}
					dirty={tab.save.kind !== "clean"}
					saving={tab.save.kind === "saving"}
					missing={tab.save.kind === "missing"}
					error={tab.save.kind === "failed" ? tab.save.message : null}
					target={tab.document.target}
					seed={tab.seed}
					takes={tab.takes}
					merging={merge !== null}
					preferences={preferences}
					onChooseTake={(take) =>
						putBack(() => chooseTake(root, tab.document.id, take))
					}
					onTry={() => startTake(tab.document)}
					onRenameTake={(take, name) =>
						putBack(() =>
							renameTake(root, tab.document.id, take, name),
						)
					}
					onDeleteTake={(take) =>
						putBack(() => deleteTake(root, tab.document.id, take))
					}
					onTakeText={(take) => takeText(root, tab.document.id, take)}
					onChange={(text) => edit(tab.document.id, text)}
					onWrote={(delta) => record(tab.document.id, delta)}
					place={places.current.get(tab.document.id) ?? null}
					onPlace={(place) =>
						places.current.set(tab.document.id, place)
					}
					onFields={setFields}
					onOutline={setOutline}
					onMarks={setMarks}
					onRestore={() => void restore(tab.document.id)}
					changed={tab.save.kind === "changed"}
					cast={cast}
					onCue={(cue) => void openCue(cue)}
					onReload={() => void reload(tab.document.id)}
					onKeepMine={() => void keepMine(tab.document.id)}
					onPreferences={onPreferences}
					onTarget={(target) => retarget(tab.document.id, target)}
				/>
			);
		}

		return (
			<article className="reader">
				<h2 className="reader__title">{tab.document.title}</h2>
				{tab.content.kind === "loading" ? (
					<p className="reader__note">Opening…</p>
				) : (
					<p className="reader__note reader__note--error">
						{tab.content.message}
					</p>
				)}
			</article>
		);
	}

	// The tabs that are drawn only while on the page. Each is keyed by its tab,
	// so moving between two overviews starts the new one empty rather than
	// showing the previous section's cards until its read comes back.
	function page(tab: Tab): ReactNode {
		switch (tab.kind) {
			case "folder":
				return (
					<FolderView
						key={tab.key}
						root={root}
						folder={tab.folder}
						reload={listing}
						merging={merge !== null}
						onSelect={(document) =>
							void openDocument(document, null, "across")
						}
						onTry={(document) => startTake(document, "across")}
						onOpenFolder={(folder) => openFolder(folder, "across")}
						// Across, so the overview it was made from stays put.
						onCreated={(document) =>
							void created(document, "across")
						}
						onRenamed={renamed}
						onFolderRenamed={renamedFolder}
						onChanged={changed}
						onMoved={() => void moved()}
						onDelete={remove}
						onDeleteFolder={removeFolder}
						onRetarget={(target) =>
							retargetFolder(tab.folder.id, target)
						}
						onRetargetDocument={retarget}
						onRetargetFolder={(id, target) =>
							retargetFolder(id, target)
						}
					/>
				);
			case "book":
				return (
					<BookView
						key={tab.key}
						root={root}
						changed={listing + written}
						onWritten={() => setBookWritten((times) => times + 1)}
						merging={merge !== null}
					/>
				);
			case "trash":
				return (
					<Trash
						key={tab.key}
						root={root}
						reload={listing}
						onChanged={() => setListing((version) => version + 1)}
						merging={merge !== null}
					/>
				);
			case "tag":
				return (
					<TagView
						key={tab.key}
						root={root}
						tag={tab.tag}
						changed={listing + written}
						live={live}
						onOpen={(document) =>
							void openDocument(document, null, "across")
						}
					/>
				);
			case "subject":
				return (
					<SubjectView
						key={tab.key}
						root={root}
						subject={tab.subject}
						changed={listing + written}
						live={live}
						onOpen={(document, seed) =>
							void openDocument(document, seed, "across")
						}
						onDraft={() =>
							void openDocument(tab.subject, null, "across")
						}
						onSubject={(document) => turnSubject(tab.key, document)}
						onRemarks={(text, failed) =>
							remark(tab.subject.id, text, failed)
						}
					/>
				);
			default:
				return null;
		}
	}

	// Only a document and a folder overview have anything to say about
	// themselves. Search and the trash are lists of other things.
	const about =
		active?.kind === "document" || active?.kind === "folder"
			? active.kind
			: null;

	// What the right sidebar is about. A document's names come from the open
	// editor rather than from the file, so a name typed into Info is
	// recognised before it reaches disk.
	const panel: About | null =
		active?.kind === "document" && fields !== null
			? {
					kind: "document",
					page: {
						id: active.document.id,
						title: active.document.title,
						trail: active.document.trail,
						names: list(fields.fields, "names"),
					},
				}
			: active?.kind === "folder"
				? { kind: "folder", trail: active.folder.trail }
				: null;

	// Compare and Merge stand where the writing stands.
	const covered = comparing !== null || (merge !== null && settling !== null);
	coveredNow.current = covered;

	function backToBinder() {
		setComparing(null);
		setSettling(null);
	}

	// The binder's own book menu, without the version, leading Compare's and
	// Merge's lists. Opening the book or a failure goes back to the binder.
	const bookMenu = (
		<BookMenu
			root={root}
			name={name}
			reload={listing}
			written={bookWritten}
			merging={merge !== null}
			onOpenBook={() => {
				backToBinder();
				openBook();
			}}
			onClose={onClose}
			onChanged={changed}
			onMessage={(message) => {
				if (message !== null) {
					backToBinder();
					setBookTrouble({ message });
				}
			}}
		/>
	);
	// Only over a document, whose dock and Escape are the ways out. Anywhere else
	// the frame is the only way around, so a saved Focus cannot strand the writer.
	const focused =
		preferences.focus && active?.kind === "document" && !covered;

	// Unmeasured counts as roomy, so a restored split does not flash folded
	// before the first measurement arrives.
	const roomy = room === 0 || fits(room, SIDE_MIN);
	const shown = layout(onPage, !focused && roomy);
	const split = standing(preferences.splitShare, room);

	// Where a tab dragged over the page would land: a side of a split, or an
	// outer third of a page that is not split.
	function zoneOf(event: DragEvent<HTMLElement>): "left" | "right" | null {
		const box = event.currentTarget.getBoundingClientRect();
		const x = event.clientX - box.left;
		if (shown.kind === "split") {
			return x < (box.width * split) / 100 ? "left" : "right";
		}
		return third(x, box.width);
	}

	// A drop on a zone. A binder row opens its tab, or finds the one it has,
	// and puts it there; opening is allowed while a merge waits.
	function land(what: Dragged, side: "left" | "right") {
		switch (what.kind) {
			case "tab":
				show(what.key, side);
				return;
			case "document":
				void openDocument(what.document, null, side);
				return;
			case "folder":
				openFolder(what.folder, side);
		}
	}

	// Where a tab's page goes, or null when it is not on the page.
	function placed(key: string | null): "full" | "left" | "right" | null {
		switch (shown.kind) {
			case "split":
				return key === shown.left
					? "left"
					: key === shown.right
						? "right"
						: null;
			default:
				return key === shown.key ? "full" : null;
		}
	}

	// One side of the page. Pressing or tabbing into the side that is not
	// current makes it current.
	function pane(key: string | null, body: ReactNode, mounted = false) {
		const where = placed(key);
		if (where === null && !mounted) {
			return null;
		}
		const enter = () =>
			setOnPage((now) =>
				now.other !== null && now.other.key === key
					? focus(now, "other")
					: now,
			);
		const classes = ["project__pane"];
		if (where === null) {
			classes.push("project__pane--hidden");
		} else if (where !== "full") {
			classes.push(`project__pane--${where}`);
			if (key === onPage.current) {
				classes.push("project__pane--current");
			}
		}
		return (
			<div
				key={key ?? "empty"}
				className={classes.join(" ")}
				onPointerDownCapture={enter}
				onFocusCapture={enter}
			>
				{body}
				{where !== null && onPage.other !== null && !focused && (
					<button
						type="button"
						className="project__dismiss"
						title="Close this side"
						aria-label="Close this side"
						onClick={() => setOnPage((now) => close(now, key))}
					>
						<Icon name="x" />
					</button>
				)}
			</div>
		);
	}

	return (
		<section className={focused ? "project project--focus" : "project"}>
			<Titlebar
				root={root}
				session={deliberate}
				sessionClock={preferences.sessionClock}
				defaultSprint={preferences.defaultSprint}
				defaultSprintUnit={preferences.defaultSprintUnit}
				onStartSession={startSession}
				onStopSession={stopSession}
				onTickSession={tickSession}
				sessions={sessions.current}
				words={words.current}
				logged={logged}
				sidebar={sidebar}
				onSidebar={(open) => {
					if (narrow) {
						setFloating(open);
					} else {
						onPreferences({ sidebar: open });
					}
				}}
				sidebarButton={sidebarButton}
				sidebarToggle={!covered}
				tabs={
					covered ? (
						<>
							<button
								type="button"
								className="quiet"
								onClick={() =>
									comparing !== null
										? setComparing(null)
										: setSettling(null)
								}
							>
								<Icon name="chevron-left" />
								Back to writing
							</button>
							{narrow && (
								<button
									ref={listButton}
									type="button"
									className="quiet"
									aria-expanded={listOpen}
									onClick={() => setListOpen((open) => !open)}
								>
									{comparing !== null
										? "Checkpoints"
										: "To settle"}
								</button>
							)}
						</>
					) : (
						<Tabs
							tabs={tabs.map(strip)}
							activeKey={activeKey}
							besideKey={onPage.other?.key ?? null}
							folded={shown.kind === "folded"}
							onActivate={setActiveKey}
							onClose={closeTab}
							onBeside={(key) =>
								setOnPage((now) => openBeside(now, key))
							}
							onSwap={() => setOnPage(swap)}
							onEndSplit={() =>
								setOnPage((now) => act(now, "end"))
							}
							onDrag={(key) =>
								drag(key === null ? null : { kind: "tab", key })
							}
						/>
					)
				}
				onSearch={openSearch}
				theme={preferences.theme}
				onTheme={(theme) => onPreferences({ theme })}
				onSettings={onSettings}
				onLogin={onLogin}
				onProfile={onProfile}
				// Merge's own list says this.
				merging={
					comparing === null && settling !== null
						? null
						: (merge?.merging ?? null)
				}
			/>

			{comparing !== null && (
				<Compare
					key={`${comparing.checkpoint}\n${comparing.slug ?? ""}\n${comparing.pick ?? ""}`}
					root={root}
					book={bookMenu}
					document={
						active?.kind === "document" ? active.document : null
					}
					scope={preferences.versionsScope}
					version={comparing.checkpoint}
					pick={comparing.pick}
					slug={comparing.slug}
					fontSize={preferences.fontSize}
					lineHeight={preferences.lineHeight}
					merging={merge !== null}
					floating={listOpen}
					onDismiss={() => setListOpen(false)}
					opener={listButton}
					onScope={(versionsScope) =>
						onPreferences({ versionsScope })
					}
					onBack={() => setComparing(null)}
					onPutBack={putBack}
				/>
			)}

			{comparing === null && merge !== null && settling !== null && (
				<Merge
					root={root}
					book={bookMenu}
					merge={merge}
					open={settling}
					onOpen={setSettling}
					choices={choices}
					onChoices={(id, chosen) =>
						setChoices((all) => ({ ...all, [id]: chosen }))
					}
					sides={sides}
					onSides={setSides}
					floating={listOpen}
					onDismiss={() => setListOpen(false)}
					opener={listButton}
					onBack={() => setSettling(null)}
					onSettle={putBack}
					onMergeChanged={readMerge}
					pieces={pieces}
					piecesFailed={piecesFailed}
				/>
			)}

			{/* Hidden rather than unmounted while comparing or merging, so
			    every editor comes back with its cursor and its undo history. */}
			<div className="project__body" hidden={covered}>
				<Sidebar
					key={root}
					hidden={!sidebar}
					floating={floating}
					onDismiss={() => setFloating(false)}
					opener={sidebarButton}
					width={preferences.sidebarWidth}
					onWidth={(sidebarWidth) => onPreferences({ sidebarWidth })}
					root={root}
					name={name}
					reload={listing + written}
					bookWritten={bookWritten}
					onWords={measured}
					unsaved={tabs.flatMap((tab) =>
						tab.kind === "document" && tab.save.kind !== "clean"
							? [tab.document.id]
							: [],
					)}
					selectedId={
						active?.kind === "document" ? active.document.id : null
					}
					selectedFolder={
						active?.kind === "folder" ? active.folder.id : null
					}
					selectedTrash={active?.kind === "trash"}
					merging={merge !== null}
					trouble={bookTrouble}
					onCompare={(checkpoint, slug) =>
						void allWritten().then(
							() =>
								setComparing({ checkpoint, pick: null, slug }),
							() => undefined,
						)
					}
					onBeforeSwitch={allWritten}
					onSwitch={putBack}
					onMergeChanged={readMerge}
					onSelect={(document, beside) =>
						void openDocument(
							document,
							null,
							beside ? "beside" : "here",
						)
					}
					onSubject={(document) => openSubject(document)}
					onTry={(document) => startTake(document)}
					onOpenFolder={(folder, beside) =>
						openFolder(folder, beside ? "beside" : "here")
					}
					onOpenTrash={(beside) =>
						openTrash(beside ? "beside" : "here")
					}
					onOpenBook={(beside) =>
						openBook(beside ? "beside" : "here")
					}
					onCreated={(document) => void created(document)}
					onRenamed={renamed}
					onFolderRenamed={renamedFolder}
					onRetarget={retarget}
					onRetargetFolder={(id, target) =>
						retargetFolder(id, target)
					}
					onChanged={changed}
					onMoved={() => void moved()}
					onDelete={remove}
					onDeleteFolder={removeFolder}
					onClose={onClose}
					onDrag={drag}
				/>
				<div className="project__main">
					<div
						ref={panes}
						className="project__panes"
						style={
							{
								"--split": `${split}%`,
							} as CSSProperties
						}
						// In the capture phase, so a tab dropped over an editor
						// is the page's and never reaches Lexical as text.
						onDragOverCapture={(event) => {
							if (dragged === null) {
								return;
							}
							event.preventDefault();
							event.stopPropagation();
							event.dataTransfer.dropEffect = "move";
							setZone(zoneOf(event));
						}}
						onDragLeave={(event) => {
							if (
								!event.currentTarget.contains(
									event.relatedTarget as Node | null,
								)
							) {
								setZone(null);
							}
						}}
						onDropCapture={(event) => {
							if (dragged === null) {
								return;
							}
							event.preventDefault();
							event.stopPropagation();
							const side = zoneOf(event);
							if (side !== null) {
								land(dragged, side);
							}
							drag(null);
						}}
					>
						{zone !== null && (
							<div
								className="project__drop"
								data-zone={zone}
								data-whole={
									shown.kind === "split" ? "" : undefined
								}
								aria-hidden="true"
							/>
						)}
						{shown.kind === "split" && (
							<div className="project__divider">
								<Grip
									side="left"
									panel={panes}
									property="--split"
									width={Math.round((room * split) / 100)}
									bounds={divider(room)}
									label="Divider between the sides"
									onWidth={(left) =>
										divide(share(left, room))
									}
									onReset={() => divide(50)}
								/>
							</div>
						)}
						{/* Every open document stays mounted and all but one
						    is hidden. An editor torn down on a tab switch
						    takes its undo history, its selection and its
						    scroll position with it. Hiding is by visibility
						    rather than display, which would drop the layout
						    box and with it the scroll position. */}
						{tabs.map((tab) =>
							tab.kind === "document"
								? pane(tab.key, documentBody(tab), true)
								: pane(tab.key, page(tab)),
						)}

						{pane(
							null,
							<p className="project__empty">
								{shown.kind === "split"
									? "Pick a tab or a binder row."
									: "Choose a document to open."}
							</p>,
						)}
					</div>

					{/* Over the panes rather than above them: the writing does
					    not move down to make room for a question about
					    something else. */}
					{retagging.kind !== "no" && (
						<p className="retag" role="status">
							{retagging.kind === "failed" ? (
								<span className="retag__said retag__said--error">
									{retagging.message}
								</span>
							) : (
								<span className="retag__said">
									{retagging.ids.length === 1
										? "One document is tagged"
										: `${retagging.ids.length} documents are tagged`}{" "}
									<span className="retag__tag">
										{retagging.from}
									</span>
									. Retag them{" "}
									<span className="retag__tag">
										{retagging.to}
									</span>
									?
								</span>
							)}

							{retagging.kind === "asking" && (
								<button
									type="button"
									className="retag__do"
									onClick={() =>
										void retagAll(
											retagging.from,
											retagging.to,
											retagging.ids,
										)
									}
								>
									Retag
								</button>
							)}

							<button
								type="button"
								className="retag__do"
								disabled={retagging.kind === "writing"}
								onClick={dismiss}
							>
								{retagging.kind === "asking"
									? "Leave"
									: "Close"}
							</button>
						</p>
					)}
				</div>

				<RightSidebar
					open={preferences.rightSidebar}
					fields={about === "document" ? fields : folderFields}
					outline={about === "document" ? outline : null}
					marks={about === "document" ? marks : null}
					trouble={trouble ?? aiming}
					about={panel}
					root={root}
					changed={listing + written}
					live={live}
					page={active?.kind === "document" ? active.document : null}
					text={
						active?.kind === "document" &&
						active.content.kind === "ready"
							? active.content.text
							: ""
					}
					folder={active?.kind === "folder" ? active.folder : null}
					onTarget={(target) =>
						active?.kind === "folder"
							? retargetFolder(active.folder.id, target)
							: active?.kind === "document"
								? retarget(active.document.id, target)
								: undefined
					}
					merging={merge !== null}
					sessions={sessions.current}
					words={words.current}
					width={preferences.rightSidebarWidth}
					onWidth={(rightSidebarWidth) =>
						onPreferences({ rightSidebarWidth })
					}
					logged={logged}
					unrecorded={unrecordedCount > 0}
					tab={preferences.rightSidebarTab}
					onTab={(tab) =>
						onPreferences({
							rightSidebar: true,
							rightSidebarTab: tab,
						})
					}
					versionsScope={preferences.versionsScope}
					onVersionsScope={(versionsScope) =>
						onPreferences({ versionsScope })
					}
					enclosing={enclosingRepository}
					merge={merge}
					pieces={pieces}
					piecesFailed={piecesFailed}
					onMergeChanged={readMerge}
					onSettle={setSettling}
					onBeforeKeep={allWritten}
					onCompare={(checkpoint, document) =>
						// A tab that could not be written shows its own error.
						void allWritten().then(
							() =>
								setComparing({
									checkpoint: checkpoint.id,
									pick: document ?? null,
									slug: null,
								}),
							() => undefined,
						)
					}
					onPutBack={putBack}
					// The panel speaks for the current side, so what it opens
					// goes across, as a link inside that side would.
					onOpen={(document, seed) =>
						void openDocument(document, seed, "across")
					}
					asked={asked}
					// Search is the way to a document, not a link from one, so a
					// hit takes the current side's place.
					onFound={(document, seed) =>
						void openDocument(document, seed)
					}
					onOpenTag={(tag) => void openTag(tag, "across")}
					onSubject={() => {
						if (active?.kind === "document") {
							openSubject(active.document, "across");
						}
					}}
					onClose={() => onPreferences({ rightSidebar: false })}
				/>
			</div>

			{asking !== null && (
				<Overwrite names={asking.names} onAnswer={asking.answer} />
			)}
		</section>
	);
}
