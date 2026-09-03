import Icon from "./Icon";
import Info from "./Info";
import Synopsis from "./Synopsis";
import type { FieldsHandle } from "./fields";
import type { RightSidebarTab } from "./types";

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
	tab: RightSidebarTab;
	onTab: (tab: RightSidebarTab) => void;
	onClose: () => void;
};

export default function RightSidebar({ fields, tab, onTab, onClose }: Props) {
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

			<div className="right-sidebar__body">
				{fields === null || tab === "mentions" ? (
					<p className="right-sidebar__empty">Nothing here yet.</p>
				) : tab === "synopsis" ? (
					<Synopsis {...fields} />
				) : (
					<Info {...fields} />
				)}
			</div>
		</aside>
	);
}
