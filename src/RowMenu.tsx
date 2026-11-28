// What a row in the binder or a card in a folder offers: the ⋮ on it and, where
// it answers one, the same items at a right-click. Also the shell a binder row
// draws around itself, which both kinds of row share.

import { useState, type ReactNode } from "react";
import { MERGE_HOLDS } from "./branches";
import Menu, { MenuItem, PointerMenu, type RightClick } from "./Menu";
import MoveTo from "./MoveTo";
import NameField from "./NameField";
import type { Draggable } from "./reorder";
import type { Retitling } from "./rows";
import { indent } from "./rows";
import type { Moving } from "./tree";

type Props = {
	label: string;
	root: string;
	moving: Moving;
	/** Given only for a row that can step up and down among its siblings. */
	ordering?: { index: number; count: number };
	onMove: (parentId: string, index: number) => void;
	/** Given only in the binder, where the row also answers a right-click. */
	onBeside?: () => void;
	onRename: () => void;
	/** Given only in the binder, where the row's count opens the field. */
	onTarget?: () => void;
	onDelete: () => void;
	/** Given only for a subject, which is the only kind with a page. */
	onSubject?: () => void;
	/** Given only for a document in the Manuscript. */
	onTry?: () => void;
	/** Given only for a story in a collection. */
	onTimeline?: () => void;
	merging: boolean;
};

/** The ⋮ on the row, and the same items where the row was right-clicked. */
export default function RowMenu({
	label,
	pointer,
	...rest
}: Props & { pointer?: RightClick }) {
	return (
		<>
			<Menu label={label} icon="more-vertical">
				{(close) => <Actions close={close} {...rest} />}
			</Menu>
			{pointer !== undefined && (
				<PointerMenu
					at={pointer.at}
					label={label}
					onClose={pointer.close}
				>
					{(close) => <Actions close={close} {...rest} />}
				</PointerMenu>
			)}
		</>
	);
}

// Its own component so that its state is the menu's: the items are unmounted
// when the menu closes, so a menu left showing the destinations opens on the
// actions again next time.
function Actions({
	close,
	root,
	moving,
	ordering,
	onMove,
	onBeside,
	onRename,
	onTarget,
	onDelete,
	onSubject,
	onTry,
	onTimeline,
	merging,
}: Omit<Props, "label"> & { close: () => void }) {
	const [choosing, setChoosing] = useState(false);
	const why = merging ? MERGE_HOLDS : undefined;

	if (choosing) {
		return (
			<MoveTo
				root={root}
				moving={moving}
				onBack={() => setChoosing(false)}
				onChoose={(parentId, at) => {
					close();
					onMove(parentId, at);
				}}
			/>
		);
	}

	return (
		<>
			{onBeside !== undefined && (
				<>
					<MenuItem
						onSelect={() => {
							close();
							onBeside();
						}}
					>
						Open beside
					</MenuItem>
					{onTimeline !== undefined && (
						<MenuItem
							onSelect={() => {
								close();
								onTimeline();
							}}
						>
							Timeline
						</MenuItem>
					)}
					<span className="menu__rule" />
				</>
			)}
			{ordering !== undefined && (
				<>
					<MenuItem
						disabled={merging || ordering.index === 0}
						title={why}
						onSelect={() => {
							close();
							onMove(moving.from, ordering.index - 1);
						}}
					>
						Move up
					</MenuItem>
					<MenuItem
						disabled={
							merging || ordering.index >= ordering.count - 1
						}
						title={why}
						onSelect={() => {
							close();
							onMove(moving.from, ordering.index + 1);
						}}
					>
						Move down
					</MenuItem>
				</>
			)}
			<MenuItem
				disabled={merging}
				title={why}
				onSelect={() => setChoosing(true)}
			>
				Move to…
			</MenuItem>
			<MenuItem
				disabled={merging}
				title={why}
				onSelect={() => {
					close();
					onRename();
				}}
			>
				Rename
			</MenuItem>
			{onTarget !== undefined && (
				<MenuItem
					disabled={merging}
					title={why}
					onSelect={() => {
						close();
						onTarget();
					}}
				>
					Set target…
				</MenuItem>
			)}
			{onSubject !== undefined && (
				<MenuItem
					onSelect={() => {
						close();
						onSubject();
					}}
				>
					Open subject page
				</MenuItem>
			)}
			{onTry !== undefined && (
				<MenuItem
					disabled={merging}
					title={why}
					onSelect={() => {
						close();
						onTry();
					}}
				>
					Try something
				</MenuItem>
			)}
			<MenuItem
				danger
				disabled={merging}
				title={why}
				onSelect={() => {
					close();
					onDelete();
				}}
			>
				Delete
			</MenuItem>
		</>
	);
}

type Shell = {
	depth: number;
	/** Whether it sits outside what the book counts. */
	dim: boolean;
	/** Whether it sits inside the Manuscript. */
	counted: boolean;
	/** Set only on the row being retitled. */
	renaming: Retitling | null;
	name: string;
	placeholder: string;
	/** Not given while the row's count is a field, so the field can be typed in. */
	drag: Draggable | null;
	onContextMenu: RightClick["open"];
	onRetitle: (name: string) => void;
	onCancelRetitle: () => void;
	children: ReactNode;
	/** Drawn right after the row: the add row, when a new thing lands there. */
	after: ReactNode;
};

/**
 * The item every binder row sits in, or the field in its place when it is the
 * row being retitled: a rename happens where the name already is.
 */
export function RowShell({
	depth,
	dim,
	counted,
	renaming,
	name,
	placeholder,
	drag,
	onContextMenu,
	onRetitle,
	onCancelRetitle,
	children,
	after,
}: Shell) {
	if (renaming !== null) {
		return (
			<li className="documents__item" style={indent(depth)}>
				<div className="sidebar__field">
					<NameField
						label={`New name for ${name}`}
						placeholder={placeholder}
						initial={name}
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
		<>
			<li
				className={[
					"documents__item",
					dim && "documents__item--out",
					counted && "documents__item--counted",
				]
					.filter(Boolean)
					.join(" ")}
				style={indent(depth)}
				{...drag}
				onContextMenu={onContextMenu}
			>
				{children}
			</li>
			{after}
		</>
	);
}
