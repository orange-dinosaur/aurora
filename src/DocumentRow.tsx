// One document in the sidebar: its number, its title, whether it is waiting to
// be written, and what may be done to it. It draws the field instead when it is
// the row being retitled, and is not draggable while it does.

import DocumentMenu from "./DocumentMenu";
import NameField from "./NameField";
import type { Row } from "./tree";
import type { Draggable } from "./reorder";
import type { Retitling } from "./rows";
import { indent } from "./rows";

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
	onOpen: () => void;
	onMove: (parentId: string, index: number) => void;
	onRename: () => void;
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
	onOpen,
	onMove,
	onRename,
	onDelete,
	onRetitle,
	onCancelRetitle,
}: Props) {
	const document = row.document;

	if (renaming !== null) {
		return (
			<li className="documents__item" style={indent(row.depth)}>
				<div className="sidebar__field">
					<NameField
						label={`New name for ${document.title}`}
						placeholder="Chapter 2"
						initial={document.title}
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
		<li className="documents__item" style={indent(row.depth)} {...drag}>
			<button
				type="button"
				className="document"
				aria-current={current ? "page" : undefined}
				onClick={onOpen}
			>
				<span className="document__at">
					{String(row.index + 1).padStart(2, "0")}
				</span>
				<span className="document__title">{document.title}</span>
				{/* Always in the row and faded when there is nothing to say, so
				    the title never shifts as the writer types. */}
				<span
					className="document__dirty"
					data-dirty={unsaved ? "" : undefined}
					aria-hidden="true"
				/>
			</button>
			<DocumentMenu
				label={`Actions for ${document.title}`}
				root={root}
				index={row.index}
				count={row.siblings}
				moving={{
					id: document.id,
					kind: null,
					folder: false,
					from: row.group,
				}}
				onMove={onMove}
				onRename={onRename}
				onDelete={onDelete}
			/>
		</li>
	);
}
