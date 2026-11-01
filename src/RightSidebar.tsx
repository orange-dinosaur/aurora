import { useEffect, useRef, useState } from "react";
import Grip from "./Grip";
import Icon, { type IconName } from "./Icon";
import Info from "./Info";
import { useDismissal } from "./Menu";
import { RIGHT_SIDEBAR } from "./panels";
import Mentions, { type About } from "./Mentions";
import Stats from "./Stats";
import Synopsis from "./Synopsis";
import type { MarksHandle } from "./TakeStrip";
import Versions from "./Versions";
import type { FieldsHandle } from "./fields";
import type { Sessions } from "./sessions";
import type { FolderRef } from "./tree";
import type { Seed } from "./lib/find";
import type { OutlineHandle } from "./lib/outline";
import type { MergeUnderway, Overwrites } from "./lib/versions";
import { MANUSCRIPT } from "./kinds";
import { isSubject } from "./subjects";
import type {
	ProjectDocument,
	RightSidebarTab,
	Checkpoint,
	VersionsScope,
} from "./types";

// The rail on the far right and the one panel it opens, about whatever is open.
// It serves a document and a folder overview alike, which is why it takes what
// it shows rather than reading it: `Project` knows which of the two is in front
// of the writer, and this only lays it out.

const TABS: { name: RightSidebarTab; label: string; icon: IconName }[] = [
	{ name: "synopsis", label: "Synopsis", icon: "card" },
	{ name: "info", label: "Info", icon: "tag" },
	{ name: "mentions", label: "Mentions", icon: "quote" },
	{ name: "stats", label: "Stats", icon: "bars" },
	{ name: "versions", label: "Versions", icon: "layers" },
];

// Where the panel leaves the page's width alone and floats over it, which the
// stylesheet's own query has to match.
const OVERLAY = "(max-width: 1080px)";

type Props = {
	/** Whether the panel is showing. The rail always is. */
	open: boolean;
	/** The open document's fields, or null when what is open has none yet. */
	fields: FieldsHandle | null;
	/** The open document's headings, or null for a folder overview. */
	outline: OutlineHandle | null;
	/** The open document's takes, or null when it has none. */
	marks: MarksHandle | null;
	/** What is open: a document, or the folder whose overview is showing. */
	about: About | null;
	/** Why a folder's last change was refused, when one was. */
	trouble: string | null;
	root: string;
	/** Bumped whenever the project changes on disk. */
	changed: number;
	/** The text of every open document as its tab holds it, by id. */
	live: Map<string, string>;
	/** The open document, and its text, for the panel that counts words. */
	page: ProjectDocument | null;
	text: string;
	/** The folder whose overview is showing, when it is one of those. */
	folder: FolderRef | null;
	/** A target typed over the number in the Stats tab. */
	onTarget: (target: number | null) => void;
	/** The two layers of the session as they stand. */
	sessions: Sessions;
	/** What the whole project holds, for the numbers that are not per document. */
	words: number;
	/** How wide the writer has dragged it, in pixels. */
	width: number;
	onWidth: (width: number) => void;
	/** Bumped when a session has reached the history file and kept a version. */
	logged: number;
	tab: RightSidebarTab;
	/** Opens the panel on that tab. */
	onTab: (tab: RightSidebarTab) => void;
	versionsScope: VersionsScope;
	onVersionsScope: (scope: VersionsScope) => void;
	/** A repository the project sits inside, while the writer has not been told. */
	enclosing: string | null;
	/** The merge waiting to be settled, if there is one. */
	merge: MergeUnderway | null;
	/** Reads the merge again after something here settled or ended it. */
	onMergeChanged: () => Promise<void>;
	/** Opens the merge screen on a conflicted document. */
	onSettle: (id: string) => void;
	/** Writes every open document to disk before a version is kept. */
	onBeforeKeep: () => Promise<void>;
	onCompare: (checkpoint: Checkpoint, document?: string) => void;
	/** Writes everything down, runs the put back, and reloads what it changed. */
	onPutBack: (
		task: () => Promise<unknown>,
		target?: Overwrites,
	) => Promise<void>;
	onOpen: (document: ProjectDocument, seed: Seed | null) => void;
	/** Where a tag chip goes, which only the project view can work out. */
	onOpenTag: (tag: string) => void;
	/** Opens the page about the open document, when it is a subject. */
	onSubject: () => void;
	onClose: () => void;
};

export default function RightSidebar({
	open,
	fields,
	outline,
	marks,
	about,
	trouble,
	root,
	changed,
	live,
	page,
	text,
	folder,
	onTarget,
	sessions,
	words,
	width,
	onWidth,
	logged,
	tab,
	onTab,
	versionsScope,
	onVersionsScope,
	enclosing,
	merge,
	onMergeChanged,
	onSettle,
	onBeforeKeep,
	onCompare,
	onPutBack,
	onOpen,
	onOpenTag,
	onSubject,
	onClose,
}: Props) {
	// Held here rather than passed in, so the element the grip moves is the
	// one this component drew.
	const panel = useRef<HTMLElement>(null);
	const rail = useRef<HTMLDivElement>(null);

	// Floating over the page, it goes the way the floating binder does.
	const [overlay, setOverlay] = useState(
		() => window.matchMedia(OVERLAY).matches,
	);
	useEffect(() => {
		const query = window.matchMedia(OVERLAY);
		function changed(event: MediaQueryListEvent) {
			setOverlay(event.matches);
		}
		query.addEventListener("change", changed);
		return () => query.removeEventListener("change", changed);
	}, []);
	useDismissal(open && overlay, panel, onClose, rail);

	return (
		<>
			{open && (
				<aside
					ref={panel}
					className="right-sidebar"
					aria-label="About what is open"
					style={{ width }}
				>
					<h2 className="right-sidebar__title">
						{TABS.find(({ name }) => name === tab)?.label}
					</h2>

					{/* Over the panel rather than in it, so a refused change does
					    not push the field the writer is looking at down the page. */}
					<p className="right-sidebar__trouble" role="alert">
						{trouble ?? ""}
					</p>

					<div className="right-sidebar__body">
						{tab === "versions" ? (
							<Versions
								root={root}
								page={page}
								scope={versionsScope}
								onScope={onVersionsScope}
								enclosing={enclosing}
								merge={merge}
								onMergeChanged={onMergeChanged}
								onSettle={onSettle}
								onBeforeKeep={onBeforeKeep}
								logged={logged}
								onCompare={onCompare}
								onPutBack={onPutBack}
							/>
						) : tab === "stats" ? (
							<Stats
								page={page}
								text={text}
								folder={folder}
								sessions={sessions}
								words={words}
								root={root}
								logged={logged}
								changed={changed}
								onTarget={onTarget}
							/>
						) : tab === "mentions" ? (
							about === null ? (
								<p className="right-sidebar__empty">
									Nothing here yet.
								</p>
							) : (
								<Mentions
									root={root}
									about={about}
									changed={changed}
									live={live}
									onOpen={onOpen}
								/>
							)
						) : fields === null ? (
							<p className="right-sidebar__empty">
								Nothing here yet.
							</p>
						) : tab === "synopsis" ? (
							<Synopsis
								{...fields}
								outline={outline}
								marks={marks}
							/>
						) : (
							<Info
								{...fields}
								subject={
									about?.kind === "document" &&
									isSubject(about.page.trail)
								}
								manuscript={inManuscript(about)}
								root={root}
								changed={changed}
								onOpenTag={onOpenTag}
								onSubject={onSubject}
							/>
						)}
					</div>

					<Grip
						side="right"
						panel={panel}
						width={width}
						bounds={RIGHT_SIDEBAR}
						label="Panel width"
						onWidth={onWidth}
					/>
				</aside>
			)}

			<div ref={rail} className="rail" role="group" aria-label="Panels">
				{TABS.map(({ name, label, icon }) => {
					const on = open && name === tab;

					return (
						<button
							key={name}
							type="button"
							className="rail__tile"
							aria-label={label}
							aria-pressed={on}
							title={label}
							onClick={() => (on ? onClose() : onTab(name))}
						>
							<Icon name={icon} />
						</button>
					);
				})}
			</div>
		</>
	);
}

/**
 * Whether what the panel is about is part of the book. The Manuscript itself is
 * not: a section is the project's shape, and a book with its manuscript
 * switched off is not a thing anyone means.
 */
function inManuscript(about: About | null): boolean {
	if (about === null) {
		return false;
	}

	const trail = about.kind === "document" ? about.page.trail : about.trail;
	const own = about.kind === "document" ? 0 : 1;

	return trail[0] === MANUSCRIPT && trail.length > own;
}
