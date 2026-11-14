// The ⋮ on a folder, beside the + that fills it. A section has no menu: the
// top level of a project is its shape rather than something in it.

import { useState } from "react";
import { MERGE_HOLDS } from "./branches";
import Menu, { MenuItem, PointerMenu, type RightClick } from "./Menu";
import MoveTo from "./MoveTo";
import type { Moving } from "./tree";

type Props = {
	label: string;
	root: string;
	moving: Moving;
	onMove: (parentId: string, index: number) => void;
	/** Given only in the binder, where the row also answers a right-click. */
	onBeside?: () => void;
	onRename: () => void;
	/** Given only in the binder, where the row's count opens the field. */
	onTarget?: () => void;
	onDelete: () => void;
	merging: boolean;
};

/** The ⋮ on the row, and the same items where the row was right-clicked. */
export default function FolderMenu({
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

// Its own component so that its state is the menu's, the same as a document's.
function Actions({
	close,
	root,
	moving,
	onMove,
	onBeside,
	onRename,
	onTarget,
	onDelete,
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
					<span className="menu__rule" />
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
