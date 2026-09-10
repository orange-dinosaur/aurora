// One folder in the sidebar: what it is, what may be made inside it, and what
// may be done to it. It draws the field instead when it is the row being
// retitled, because a rename happens where the name already is, and is not
// draggable while it does.

import FolderMenu from "./FolderMenu";
import { abbreviated } from "./cards";
import Icon from "./Icon";
import NameField from "./NameField";
import NewMenu from "./NewMenu";
import type { FolderKind } from "./types";
import type { Making } from "./kinds";
import { folderPlaceholder } from "./kinds";
import type { Draggable } from "./reorder";
import type { Row } from "./tree";
import type { Retitling } from "./rows";
import { indent } from "./rows";

// What stands in a folder's left-hand slot, where a document carries its
// number. Without one the name starts at the edge of the column and reads as
// the smaller thing, which a folder is not. The Manuscript's folders say which
// they are; a folder anywhere else is only a folder, so it wears the glyph.
function mark(kind: FolderKind | null) {
	switch (kind) {
		case "part":
			return "PT";
		case "chapter":
			return "CH";
		case "front-matter":
			return "FM";
		case "back-matter":
			return "BM";
		case null:
			return <Icon name="folder" className="folder__glyph" />;
	}
}

type Props = {
	root: string;
	row: Extract<Row, { kind: "folder" }>;
	/** The section it lives under, which decides what may be made inside it. */
	section: string;
	/** Whether its overview is the tab showing. */
	current: boolean;
	/** Set only on the row being retitled. */
	renaming: Retitling | null;
	drag: Draggable;
	onOpen: () => void;
	onNew: (making: Making) => void;
	onMove: (parentId: string, index: number) => void;
	onRename: () => void;
	onDelete: () => void;
	onToggle: () => void;
	onRetitle: (name: string) => void;
	onCancelRetitle: () => void;
};

export default function FolderRow({
	root,
	row,
	section,
	current,
	renaming,
	drag,
	onOpen,
	onNew,
	onMove,
	onRename,
	onDelete,
	onToggle,
	onRetitle,
	onCancelRetitle,
}: Props) {
	if (renaming !== null) {
		return (
			<li className="documents__item" style={indent(row.depth)}>
				<div className="sidebar__field">
					<NameField
						label={`New name for ${row.name}`}
						placeholder={folderPlaceholder(row.folderKind)}
						initial={row.name}
						busy={renaming.busy}
						error={renaming.error}
						onSubmit={onRetitle}
						onCancel={onCancelRetitle}
					/>
				</div>
			</li>
		);
	}

	return (
		<li
			className={
				row.dim
					? "documents__item documents__item--out"
					: "documents__item"
			}
			style={indent(row.depth)}
			{...drag}
		>
			{/* Over the row rather than inside it, for the reason the menus at
			    the other end are: the row is itself a button. A folder holding
			    nothing has nothing to open, so it draws no chevron and leaves
			    the column to the folders that do. */}
			{row.holds > 0 && (
				<button
					type="button"
					className="disclosure"
					aria-expanded={row.expanded}
					aria-label={`${row.expanded ? "Collapse" : "Expand"} ${row.name}`}
					onClick={onToggle}
				>
					<Icon
						name={row.expanded ? "chevron-down" : "chevron-right"}
						className="disclosure__glyph"
					/>
				</button>
			)}

			<button
				type="button"
				className="folder"
				aria-current={current ? "page" : undefined}
				onClick={onOpen}
			>
				<span className="folder__at">{mark(row.folderKind)}</span>
				<span className="folder__name">{row.name}</span>
				{/* Left of the menus' slot rather than in it, so the count is
				    still there when the row is hovered and they appear. */}
				<span className="folder__words">{abbreviated(row.words)}</span>
			</button>

			{/* Two menus share the right-hand end of a folder's row, so they sit
			    in a slot rather than both reaching for the same edge. */}
			<div className="documents__actions">
				<NewMenu
					label={`New in ${row.name}`}
					kind={row.folderKind}
					section={section}
					onChoose={onNew}
				/>
				<FolderMenu
					label={`Actions for ${row.name}`}
					root={root}
					moving={{
						id: row.id,
						kind: row.folderKind,
						folder: true,
						from: row.group,
					}}
					onMove={onMove}
					onRename={onRename}
					onDelete={onDelete}
				/>
			</div>
		</li>
	);
}
