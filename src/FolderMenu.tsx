// The ⋮ on a folder, beside the + that fills it. A section has no menu: the
// top level of a project is its shape rather than something in it.

import { useState } from "react";
import { MERGE_HOLDS } from "./branches";
import Menu, { MenuItem } from "./Menu";
import MoveTo from "./MoveTo";
import type { Moving } from "./tree";

type Props = {
	label: string;
	root: string;
	moving: Moving;
	onMove: (parentId: string, index: number) => void;
	onRename: () => void;
	onDelete: () => void;
	merging: boolean;
};

export default function FolderMenu({ label, ...rest }: Props) {
	return (
		<Menu label={label} icon="more-vertical">
			{(close) => <Actions close={close} {...rest} />}
		</Menu>
	);
}

// Its own component so that its state is the menu's, the same as a document's.
function Actions({
	close,
	root,
	moving,
	onMove,
	onRename,
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
