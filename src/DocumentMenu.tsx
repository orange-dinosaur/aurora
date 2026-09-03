import { useState } from "react";
import Menu, { MenuItem } from "./Menu";
import MoveTo from "./MoveTo";
import type { Moving } from "./tree";

type Props = {
	label: string;
	root: string;
	// Where this document sits among the others beside it, which is what says
	// whether there is anywhere left to move it.
	index: number;
	count: number;
	moving: Moving;
	onMove: (parentId: string, index: number) => void;
	onRename: () => void;
	onDelete: () => void;
};

export default function DocumentMenu({ label, ...rest }: Props) {
	return (
		<Menu label={label} icon="more-vertical">
			{(close) => <Actions close={close} {...rest} />}
		</Menu>
	);
}

// Its own component so that its state is the menu's: the items are unmounted
// when the menu closes, so a menu left showing the destinations opens on the
// actions again next time.
function Actions({
	close,
	root,
	index,
	count,
	moving,
	onMove,
	onRename,
	onDelete,
}: Omit<Props, "label"> & { close: () => void }) {
	const [choosing, setChoosing] = useState(false);

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
				disabled={index === 0}
				onSelect={() => {
					close();
					onMove(moving.from, index - 1);
				}}
			>
				Move up
			</MenuItem>
			<MenuItem
				disabled={index >= count - 1}
				onSelect={() => {
					close();
					onMove(moving.from, index + 1);
				}}
			>
				Move down
			</MenuItem>
			<MenuItem onSelect={() => setChoosing(true)}>Move to…</MenuItem>
			<MenuItem
				onSelect={() => {
					close();
					onRename();
				}}
			>
				Rename
			</MenuItem>
			<MenuItem
				danger
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
