import { useEffect, useRef, useState } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import Icon from "./Icon";
import type { IconName } from "./Icon";
import { outOfView } from "./lib/overflow";
import { MenuItem, PointerMenu } from "./Menu";

/**
 * One tab as the strip shows it. The key is what the strip hands back; only
 * the project view knows what it opens.
 */
export type TabView = {
	key: string;
	kind: "document" | "folder" | "book" | "trash" | "tag" | "subject";
	/** The section a document sits in. Nothing else has one. */
	folder: string | null;
	title: string;
	dirty: boolean;
};

// What a tab is, said once at its head. A document goes unmarked: it is what
// the strip is mostly full of.
const MARKS: Record<TabView["kind"], IconName | null> = {
	document: null,
	folder: "folder",
	book: "book",
	trash: "trash",
	tag: "tag",
	subject: "user",
};

// How far an arrow moves the row: about one full tab.
const NUDGE = 180;

type Props = {
	tabs: TabView[];
	activeKey: string | null;
	/** The tab on the other side of a split, lit less strongly. */
	besideKey: string | null;
	/** Whether that side is folded away for want of room. */
	folded: boolean;
	onActivate: (key: string) => void;
	onClose: (key: string) => void;
	/** Puts the tab on the side the writer is not in. */
	onBeside: (key: string) => void;
	/** The two sides trade places. */
	onSwap: () => void;
	/** Ends the split, keeping the current side. */
	onEndSplit: () => void;
};

export default function Tabs({
	tabs,
	activeKey,
	besideKey,
	folded,
	onActivate,
	onClose,
	onBeside,
	onSwap,
	onEndSplit,
}: Props) {
	const row = useRef<HTMLUListElement>(null);
	// The tab right-clicked, and where. Kept after the menu shuts, so its items
	// still have a tab to name while they fade.
	const [asked, setAsked] = useState<{
		key: string;
		at: { x: number; y: number };
		open: boolean;
	} | null>(null);
	const split = besideKey !== null;
	const active = useRef<HTMLLIElement>(null);
	const [before, setBefore] = useState(false);
	const [after, setAfter] = useState(false);
	const showing = tabs.length > 0;
	const titles = tabs.map((tab) => tab.title).join("\n");

	function fit() {
		if (row.current !== null) {
			const { scrollLeft, scrollWidth, clientWidth } = row.current;
			const hidden = outOfView(scrollLeft, scrollWidth, clientWidth);
			setBefore(hidden.before);
			setAfter(hidden.after);
		}
	}

	function ask(event: ReactMouseEvent<HTMLElement>, key: string) {
		event.preventDefault();
		// The menu key reports no pointer, so the menu hangs under the tab.
		const box = event.currentTarget.getBoundingClientRect();
		const keyed = event.clientX === 0 && event.clientY === 0;
		setAsked({
			key,
			at: keyed
				? { x: box.left, y: box.bottom }
				: { x: event.clientX, y: event.clientY },
			open: true,
		});
	}

	function nudge(by: number) {
		const still = window.matchMedia("(prefers-reduced-motion: reduce)");
		row.current?.scrollBy({
			left: by,
			behavior: still.matches ? "auto" : "smooth",
		});
	}

	// The row hides its scrollbar, so a tab opened past the edge has to bring
	// itself in. Asking costs nothing when it is already in view.
	useEffect(() => {
		active.current?.scrollIntoView({ block: "nearest", inline: "nearest" });
	}, [activeKey]);

	// The bar changes width with the window and with the binder. The bundled
	// font arriving changes how wide the tabs are without resizing the row.
	useEffect(() => {
		if (row.current === null) {
			return;
		}

		const watcher = new ResizeObserver(fit);
		watcher.observe(row.current);
		void document.fonts.ready.then(fit);
		return () => watcher.disconnect();
	}, [showing]);

	useEffect(fit, [titles]);

	if (!showing) {
		return null;
	}

	const overflowing = before || after;

	return (
		<div className="tabs-wrap">
			{overflowing && (
				<button
					type="button"
					className="nudge"
					aria-label="Earlier tabs"
					disabled={!before}
					onClick={() => nudge(-NUDGE)}
				>
					<Icon name="chevron-left" />
				</button>
			)}

			<ul
				ref={row}
				className="tabs"
				data-before={before ? "" : undefined}
				data-after={after ? "" : undefined}
				onScroll={fit}
				onWheel={(event) => {
					event.currentTarget.scrollLeft += event.deltaY;
				}}
			>
				{tabs.map((tab) => {
					const mark = MARKS[tab.kind];

					return (
						<li
							key={tab.key}
							ref={tab.key === activeKey ? active : null}
							className="tab"
							data-active={tab.key === activeKey ? "" : undefined}
							data-beside={
								tab.key !== besideKey
									? undefined
									: folded
										? "folded"
										: ""
							}
							onContextMenu={(event) => ask(event, tab.key)}
						>
							{/* Laid over the pill's edge, so a tab does not change
							    width the moment it is typed into. */}
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
								title={
									tab.folder === null
										? tab.title
										: `${tab.folder}/${tab.title}`
								}
								onClick={() => onActivate(tab.key)}
							>
								{mark !== null && (
									<Icon name={mark} className="tab__mark" />
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

			{overflowing && (
				<button
					type="button"
					className="nudge"
					aria-label="Later tabs"
					disabled={!after}
					onClick={() => nudge(NUDGE)}
				>
					<Icon name="chevron-right" />
				</button>
			)}

			<PointerMenu
				at={asked?.open === true ? asked.at : null}
				label="Tab"
				onClose={() =>
					setAsked((now) =>
						now === null ? now : { ...now, open: false },
					)
				}
			>
				{(close) => {
					if (asked === null) {
						return null;
					}
					const { key } = asked;
					const onSide =
						split && (key === activeKey || key === besideKey);
					function pick(action: () => void) {
						close();
						action();
					}

					return (
						<>
							{onSide ? (
								<MenuItem onSelect={() => pick(onSwap)}>
									Swap sides
								</MenuItem>
							) : (
								<MenuItem
									onSelect={() => pick(() => onBeside(key))}
								>
									Open beside
								</MenuItem>
							)}
							{split && (
								<MenuItem onSelect={() => pick(onEndSplit)}>
									Close split
								</MenuItem>
							)}
							<MenuItem onSelect={() => pick(() => onClose(key))}>
								Close
							</MenuItem>
						</>
					);
				}}
			</PointerMenu>
		</div>
	);
}
