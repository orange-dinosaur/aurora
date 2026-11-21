// One folder in the sidebar: what it is, what may be made inside it, and what
// may be done to it. It draws the field instead when it is the row being
// retitled, because a rename happens where the name already is, and is not
// draggable while it does.

import { useState } from "react";
import { useRightClick } from "./Menu";
import Icon from "./Icon";
import NewMenu from "./NewMenu";
import type { Making } from "./kinds";
import { folderPlaceholder } from "./kinds";
import RowMenu, { RowShell } from "./RowMenu";
import type { Draggable } from "./reorder";
import type { Row } from "./tree";
import { inManuscript } from "./tree";
import { abbreviated, type Retitling } from "./rows";
import { progress } from "./RowProgress";
import { TargetField, type Aim } from "./Target";

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
	merging: boolean;
	/** Opens its overview, beside on the side the writer is not in. */
	onOpen: (beside: boolean) => void;
	onNew: (making: Making) => void;
	onMove: (parentId: string, index: number) => void;
	onRename: () => void;
	onTarget: Aim;
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
	merging,
	onOpen,
	onNew,
	onMove,
	onRename,
	onTarget,
	onDelete,
	onToggle,
	onRetitle,
	onCancelRetitle,
}: Props) {
	const pointer = useRightClick();
	const [aiming, setAiming] = useState(false);

	return (
		<RowShell
			depth={row.depth}
			dim={row.dim}
			counted={inManuscript(row)}
			renaming={renaming}
			name={row.name}
			placeholder={folderPlaceholder(row.folderKind)}
			drag={aiming ? null : drag}
			onContextMenu={pointer.open}
			onRetitle={onRetitle}
			onCancelRetitle={onCancelRetitle}
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
					<Icon name="chevron-down" className="disclosure__glyph" />
				</button>
			)}

			{aiming ? (
				// A form cannot sit inside a button, so the row is drawn
				// without one while its count is a field.
				<div
					className="folder"
					data-kind={row.folderKind ?? undefined}
					aria-current={current ? "page" : undefined}
				>
					<span className="folder__name">{row.name}</span>
					<TargetField
						compact
						target={row.target}
						onTarget={onTarget}
						onDone={() => setAiming(false)}
					/>
				</div>
			) : (
				<button
					type="button"
					className="folder"
					data-kind={row.folderKind ?? undefined}
					aria-current={current ? "page" : undefined}
					onClick={() => onOpen(false)}
				>
					<span className="folder__name">{row.name}</span>
					{/* Left of the menus' slot rather than in it, so the two can
					    appear together when the row is hovered. */}
					<span className="folder__words">
						{abbreviated(row.words)}
					</span>
				</button>
			)}
			{progress(row.words, row.target)}

			{/* Two menus share the right-hand end of a folder's row, so they sit
			    in a slot rather than both reaching for the same edge. */}
			<div className="documents__actions">
				<NewMenu
					label={`New in ${row.name}`}
					kind={row.folderKind}
					section={section}
					merging={merging}
					onChoose={onNew}
				/>
				<RowMenu
					label={`Actions for ${row.name}`}
					pointer={pointer}
					root={root}
					moving={{
						id: row.id,
						kind: row.folderKind,
						folder: true,
						from: row.group,
					}}
					onMove={onMove}
					onBeside={() => onOpen(true)}
					onRename={onRename}
					onTarget={() => setAiming(true)}
					onDelete={onDelete}
					merging={merging}
				/>
			</div>
		</RowShell>
	);
}
