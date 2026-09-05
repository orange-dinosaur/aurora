import Icon from "./Icon";
import Info from "./Info";
import Mentions, { type About } from "./Mentions";
import Stats from "./Stats";
import Synopsis from "./Synopsis";
import type { FieldsHandle } from "./fields";
import type { Limit, Sessions } from "./sessions";
import type { FolderRef } from "./tree";
import type { Seed } from "./find";
import type { OutlineHandle } from "./outline";
import { isSubject } from "./subjects";
import type { ProjectDocument, RightSidebarTab, SprintUnit } from "./types";

// The panel on the far side of the writing from the sidebar, about whatever is
// open. It serves a document and a folder overview alike, which is why it takes
// what it shows rather than reading it: `Project` knows which of the two is in
// front of the writer, and this only lays it out.

const TABS: { name: RightSidebarTab; label: string }[] = [
	{ name: "synopsis", label: "Synopsis" },
	{ name: "info", label: "Info" },
	{ name: "mentions", label: "Mentions" },
	{ name: "stats", label: "Stats" },
];

type Props = {
	/** The open document's fields, or null when what is open has none yet. */
	fields: FieldsHandle | null;
	/** The open document's headings, or null for a folder overview. */
	outline: OutlineHandle | null;
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
	/** What the sprint field starts on, from the writer's preferences. */
	defaultSprint: number | null;
	defaultSprintUnit: SprintUnit;
	/** Opens a deliberate session with something to reach, and closes one. */
	onStartSprint: (limit: Limit) => void;
	onStopSession: () => void;
	/** Bumped when a session has reached the history file. */
	logged: number;
	tab: RightSidebarTab;
	onTab: (tab: RightSidebarTab) => void;
	onOpen: (document: ProjectDocument, seed: Seed | null) => void;
	/** Where a tag chip goes, which only the project view can work out. */
	onOpenTag: (tag: string) => void;
	onClose: () => void;
};

export default function RightSidebar({
	fields,
	outline,
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
	defaultSprint,
	defaultSprintUnit,
	onStartSprint,
	onStopSession,
	logged,
	tab,
	onTab,
	onOpen,
	onOpenTag,
	onClose,
}: Props) {
	return (
		<aside className="right-sidebar" aria-label="About what is open">
			<div className="right-sidebar__tabs">
				{TABS.map(({ name, label }) => (
					<button
						key={name}
						type="button"
						className={
							name === tab
								? "right-sidebar__tab right-sidebar__tab--on"
								: "right-sidebar__tab"
						}
						aria-pressed={name === tab}
						onClick={() => onTab(name)}
					>
						{label}
					</button>
				))}
				<button
					type="button"
					className="right-sidebar__close"
					aria-label="Hide the right sidebar"
					onClick={onClose}
				>
					<Icon name="x" />
				</button>
			</div>

			{/* Over the panel rather than in it, so a refused change does not
			    push the field the writer is looking at down the page. */}
			<p className="right-sidebar__trouble" role="alert">
				{trouble ?? ""}
			</p>

			<div className="right-sidebar__body">
				{tab === "stats" ? (
					<Stats
						page={page}
						text={text}
						folder={folder}
						sessions={sessions}
						words={words}
						defaultSprint={defaultSprint}
						defaultSprintUnit={defaultSprintUnit}
						root={root}
						logged={logged}
						changed={changed}
						onTarget={onTarget}
						onStartSprint={onStartSprint}
						onStopSession={onStopSession}
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
					<p className="right-sidebar__empty">Nothing here yet.</p>
				) : tab === "synopsis" ? (
					<Synopsis {...fields} outline={outline} />
				) : (
					<Info
						{...fields}
						subject={
							about?.kind === "document" &&
							isSubject(about.page.trail)
						}
						root={root}
						changed={changed}
						onOpenTag={onOpenTag}
					/>
				)}
			</div>
		</aside>
	);
}
