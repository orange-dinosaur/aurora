import Menu, { MenuItem } from "./Menu";

type Props = {
	label: string;
	// Where this document sits among the others beside it, which is what says
	// whether there is anywhere left to move it.
	index: number;
	count: number;
	onMove: (index: number) => void;
	onRename: () => void;
	onDelete: () => void;
};

export default function DocumentMenu({
	label,
	index,
	count,
	onMove,
	onRename,
	onDelete,
}: Props) {
	return (
		<Menu label={label} icon="more-vertical">
			{(close) => (
				<>
					<MenuItem
						disabled={index === 0}
						onSelect={() => {
							close();
							onMove(index - 1);
						}}
					>
						Move up
					</MenuItem>
					<MenuItem
						disabled={index >= count - 1}
						onSelect={() => {
							close();
							onMove(index + 1);
						}}
					>
						Move down
					</MenuItem>
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
			)}
		</Menu>
	);
}
