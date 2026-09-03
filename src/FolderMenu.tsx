// The ⋮ on a folder, beside the + that fills it. A section has no menu: the
// top level of a project is its shape rather than something in it.

import Menu, { MenuItem } from "./Menu";

type Props = {
	label: string;
	onRename: () => void;
};

export default function FolderMenu({ label, onRename }: Props) {
	return (
		<Menu label={label} icon="more-vertical">
			{(close) => (
				<MenuItem
					onSelect={() => {
						close();
						onRename();
					}}
				>
					Rename
				</MenuItem>
			)}
		</Menu>
	);
}
