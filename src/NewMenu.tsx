// The + that makes something new, offering only what belongs where it sits.

import Menu, { MenuItem } from "./Menu";
import { creatable } from "./kinds";
import type { Making } from "./kinds";
import type { FolderKind } from "./types";

type Props = {
	label: string;
	/** The kind of the folder the + sits in, and the section it lives under. */
	kind: FolderKind | null;
	section: string;
	text?: string;
	className?: string;
	onChoose: (making: Making) => void;
};

export default function NewMenu({
	label,
	kind,
	section,
	text,
	className,
	onChoose,
}: Props) {
	return (
		<Menu label={label} icon="plus" text={text} className={className}>
			{(close) =>
				creatable(kind, section).map((making) => (
					<MenuItem
						key={making.label}
						onSelect={() => {
							close();
							onChoose(making);
						}}
					>
						{making.label}
					</MenuItem>
				))
			}
		</Menu>
	);
}
