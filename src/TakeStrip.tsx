// The thin line above the text that switches between a document's takes.

import type { TakeView } from "./bindings/TakeView";

type Props = {
	takes: TakeView[];
	/** Set while a merge is under way, when takes cannot change. */
	merging: boolean;
	onChoose: (take: string) => void;
	onTry: () => void;
};

export default function TakeStrip({ takes, merging, onChoose, onTry }: Props) {
	return (
		<div className="takes">
			<ul className="takes__list" aria-label="Takes">
				{takes.map((take) => (
					<li key={take.id} className="takes__item">
						<button
							type="button"
							className="takes__button"
							aria-current={take.chosen ? "true" : undefined}
							disabled={merging}
							onClick={() => onChoose(take.id)}
						>
							{take.name}
						</button>
					</li>
				))}
			</ul>
			<button
				type="button"
				className="takes__button takes__try"
				disabled={merging}
				onClick={onTry}
			>
				Try something
			</button>
		</div>
	);
}
