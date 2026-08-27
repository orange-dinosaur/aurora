type Props = {
	// One entry per open tab, whatever the tab holds. The key is what the
	// strip hands back; only the project view knows what it means.
	tabs: {
		key: string;
		folder: string | null;
		title: string;
		dirty: boolean;
	}[];
	activeKey: string | null;
	onActivate: (key: string) => void;
	onClose: (key: string) => void;
};

export default function Tabs({ tabs, activeKey, onActivate, onClose }: Props) {
	if (tabs.length === 0) {
		return null;
	}

	return (
		<ul className="tabs">
			{tabs.map((tab) => (
				<li
					key={tab.key}
					className="tab"
					data-active={tab.key === activeKey ? "" : undefined}
				>
					<button
						type="button"
						className="tab__title"
						aria-current={
							tab.key === activeKey ? "page" : undefined
						}
						onClick={() => onActivate(tab.key)}
					>
						{tab.folder !== null && (
							<span className="tab__folder">{tab.folder}/</span>
						)}
						{tab.title}
					</button>
					{/* Always in the strip and only ever faded in, so a tab
					    does not change width the moment it is typed into. */}
					<span
						className="tab__dirty"
						data-dirty={tab.dirty ? "" : undefined}
						aria-hidden="true"
					/>
					<button
						type="button"
						className="tab__close"
						aria-label={`Close ${tab.title}`}
						onClick={() => onClose(tab.key)}
					>
						×
					</button>
				</li>
			))}
		</ul>
	);
}
