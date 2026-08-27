import type { ProjectDocument } from "./types";

type Props = {
	documents: ProjectDocument[];
	activeId: string | null;
	onActivate: (id: string) => void;
	onClose: (id: string) => void;
};

export default function Tabs({
	documents,
	activeId,
	onActivate,
	onClose,
}: Props) {
	if (documents.length === 0) {
		return null;
	}

	return (
		<ul className="tabs">
			{documents.map((document) => (
				<li
					key={document.id}
					className="tab"
					data-active={document.id === activeId ? "" : undefined}
				>
					<button
						type="button"
						className="tab__title"
						aria-current={
							document.id === activeId ? "page" : undefined
						}
						onClick={() => onActivate(document.id)}
					>
						<span className="tab__folder">{document.folder}/</span>
						{document.title}
					</button>
					<button
						type="button"
						className="tab__close"
						aria-label={`Close ${document.title}`}
						onClick={() => onClose(document.id)}
					>
						×
					</button>
				</li>
			))}
		</ul>
	);
}
