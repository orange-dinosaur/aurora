// One document in the sidebar: its title, whether it is waiting to be written,
// and what may be done to it. It draws the field instead when it is the row
// being retitled, and is not draggable while it does.

import { useState } from "react";
import { abbreviated } from "./cards";
import DocumentMenu from "./DocumentMenu";
import { documentPlaceholder } from "./kinds";
import { useRightClick } from "./Menu";
import NameField from "./NameField";
import { isSubject } from "./subjects";
import { canTry } from "./takes";
import type { Row } from "./tree";
import { inManuscript } from "./tree";
import type { Draggable } from "./reorder";
import type { Retitling } from "./rows";
import { indent } from "./rows";
import { progress } from "./RowProgress";
import { TargetField, type Aim } from "./Target";

type Props = {
	root: string;
	row: Extract<Row, { kind: "document" }>;
	/** Whether it is the tab showing. */
	current: boolean;
	/** Whether it holds changes that have not reached disk yet. */
	unsaved: boolean;
	/** Set only on the row being retitled. */
	renaming: Retitling | null;
	drag: Draggable;
	merging: boolean;
	/** Opens it, beside on the side the writer is not in. */
	onOpen: (beside: boolean) => void;
	/** Opens the page about it, which only a subject has. */
	onSubject: () => void;
	onTry: () => void;
	onMove: (parentId: string, index: number) => void;
	onRename: () => void;
	onTarget: Aim;
	onDelete: () => void;
	onRetitle: (name: string) => void;
	onCancelRetitle: () => void;
};

export default function DocumentRow({
	root,
	row,
	current,
	unsaved,
	renaming,
	drag,
	merging,
	onOpen,
	onSubject,
	onTry,
	onMove,
	onRename,
	onTarget,
	onDelete,
	onRetitle,
	onCancelRetitle,
}: Props) {
	const document = row.document;
	const pointer = useRightClick();
	const [aiming, setAiming] = useState(false);

	const title = (
		<>
			<span className="document__title">{document.title}</span>
			{/* Always in the row and faded when there is nothing to say, so
			    the title never shifts as the writer types. */}
			<span
				className="document__dirty"
				data-dirty={unsaved ? "" : undefined}
				aria-hidden="true"
			/>
			{unsaved && <span className="visually-hidden"> (unsaved)</span>}
		</>
	);

	if (renaming !== null) {
		return (
			<li className="documents__item" style={indent(row.depth)}>
				<div className="sidebar__field">
					<NameField
						label={`New name for ${document.title}`}
						placeholder={documentPlaceholder(document.trail[0])}
						initial={document.title}
						busy={renaming.busy}
						working="Renaming…"
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
			className={[
				"documents__item",
				row.dim && "documents__item--out",
				inManuscript(row) && "documents__item--counted",
			]
				.filter(Boolean)
				.join(" ")}
			style={indent(row.depth)}
			{...(aiming ? {} : drag)}
			onContextMenu={pointer.open}
		>
			{aiming ? (
				// A form cannot sit inside a button, so the row is drawn
				// without one while its count is a field.
				<div
					className="document"
					aria-current={current ? "page" : undefined}
				>
					{title}
					<TargetField
						compact
						target={document.target}
						onTarget={onTarget}
						onDone={() => setAiming(false)}
					/>
				</div>
			) : (
				<button
					type="button"
					className="document"
					aria-current={current ? "page" : undefined}
					onClick={() => onOpen(false)}
				>
					{title}
					<span className="document__words">
						{abbreviated(row.words)}
					</span>
				</button>
			)}
			{progress(row.words, document.target)}
			<DocumentMenu
				label={`Actions for ${document.title}`}
				pointer={pointer}
				root={root}
				index={row.index}
				count={row.siblings}
				moving={{
					id: document.id,
					kind: null,
					folder: false,
					from: row.group,
				}}
				onSubject={isSubject(document.trail) ? onSubject : undefined}
				onTry={canTry(document.trail) ? onTry : undefined}
				merging={merging}
				onMove={onMove}
				onBeside={() => onOpen(true)}
				onRename={onRename}
				onTarget={() => setAiming(true)}
				onDelete={onDelete}
			/>
		</li>
	);
}
