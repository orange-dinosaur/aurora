// The + that makes something new, offering only what belongs where it sits.

import Menu, { MenuItem } from "./Menu";
import Icon from "./Icon";
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
	const offered = creatable(kind, section);

	// A chapter holds scenes and nothing else, so there is nothing to choose
	// between: the + asks for the name straight away rather than opening a
	// menu of one.
	if (offered.length === 1) {
		const only = offered[0];
		// Still wrapped: the surfaces place and reveal a + by its menu, and a
		// bare button would sit somewhere else in the row.
		return (
			<div className="menu">
				<button
					type="button"
					className={className ?? "menu__trigger"}
					aria-label={only.label}
					onClick={() => onChoose(only)}
				>
					<Icon name="plus" />
					{text}
				</button>
			</div>
		);
	}

	return (
		<Menu label={label} icon="plus" text={text} className={className}>
			{(close) =>
				offered.map((making) => (
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
