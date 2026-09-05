import { useEffect, useRef } from "react";
import Icon from "./Icon";
import type { IconName } from "./Icon";

/**
 * One tab as the strip shows it. The key is what the strip hands back; only
 * the project view knows what it opens.
 */
export type TabView = {
	key: string;
	kind: "document" | "folder" | "trash" | "search" | "tag" | "subject";
	/** The section a document sits in. Nothing else has one. */
	folder: string | null;
	title: string;
	dirty: boolean;
};

// What a tab is, said once at its head. A document goes unmarked: it is what
// the strip is mostly full of, and its section already sits in front of the
// title.
const MARKS: Record<TabView["kind"], IconName | null> = {
	document: null,
	folder: "folder",
	trash: "trash",
	search: "search",
	tag: "tag",
	subject: "user",
};

type Props = {
	tabs: TabView[];
	activeKey: string | null;
	onActivate: (key: string) => void;
	onClose: (key: string) => void;
};

export default function Tabs({ tabs, activeKey, onActivate, onClose }: Props) {
	const active = useRef<HTMLLIElement>(null);

	// The strip hides its scrollbar, so a tab opened past the edge has to
	// bring itself in. Asking costs nothing when it is already in view.
	useEffect(() => {
		active.current?.scrollIntoView({ block: "nearest", inline: "nearest" });
	}, [activeKey]);

	if (tabs.length === 0) {
		return null;
	}

	return (
		<ul className="tabs">
			{tabs.map((tab) => {
				const mark = MARKS[tab.kind];

				return (
					<li
						key={tab.key}
						ref={tab.key === activeKey ? active : null}
						className="tab"
						data-active={tab.key === activeKey ? "" : undefined}
					>
						{/* Always in the strip and only ever faded in, so a tab
					    does not change width the moment it is typed into. */}
						<span
							className="tab__dirty"
							data-dirty={tab.dirty ? "" : undefined}
							aria-hidden="true"
						/>
						<button
							type="button"
							className="tab__title"
							aria-current={
								tab.key === activeKey ? "page" : undefined
							}
							onClick={() => onActivate(tab.key)}
						>
							{mark !== null && (
								<Icon name={mark} className="tab__mark" />
							)}
							{tab.folder !== null && (
								<span className="tab__folder">
									{tab.folder}/
								</span>
							)}
							{tab.title}
						</button>
						<button
							type="button"
							className="tab__close"
							aria-label={`Close ${tab.title}`}
							onClick={() => onClose(tab.key)}
						>
							<Icon name="x" />
						</button>
					</li>
				);
			})}
		</ul>
	);
}
