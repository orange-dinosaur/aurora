import Icon from "./Icon";
import Info from "./Info";
import Mentions, { type About } from "./Mentions";
import Synopsis from "./Synopsis";
import type { FieldsHandle } from "./fields";
import type { Seed } from "./find";
import type { OutlineHandle } from "./outline";
import { isSubject } from "./subjects";
import type { ProjectDocument, RightSidebarTab } from "./types";

// The panel on the far side of the writing from the sidebar, about whatever is
// open. It serves a document and a folder overview alike, which is why it takes
// what it shows rather than reading it: `Project` knows which of the two is in
// front of the writer, and this only lays it out.

const TABS: { name: RightSidebarTab; label: string }[] = [
	{ name: "synopsis", label: "Synopsis" },
	{ name: "info", label: "Info" },
	{ name: "mentions", label: "Mentions" },
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
				{tab === "mentions" ? (
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
