// A binder row's count, with a ring left of it when the row has a word target.
// The ring has a fixed slot, so rings stand in one column whether or not the
// rows around them have one.

import { RING, aimed, arc, band, reached } from "./cards";
import { abbreviated } from "./rows";

type Props = {
	words: number;
	pages: number | null;
	target: number | null;
};

export function Count({ words, pages, target }: Props) {
	const share = reached(pages ?? words, target);
	const said = aimed(words, target, pages);

	return (
		<span className="row__count" title={said ?? undefined}>
			<span className="row__ring">
				{share !== null && (
					<svg
						viewBox="0 0 12 12"
						data-band={band(share)}
						aria-hidden="true"
					>
						<circle cx="6" cy="6" r={RING} />
						<circle
							cx="6"
							cy="6"
							r={RING}
							strokeDasharray={arc(share)}
							transform="rotate(-90 6 6)"
						/>
					</svg>
				)}
			</span>
			<span
				className="row__n"
				aria-hidden={said === null ? undefined : true}
			>
				{abbreviated(words, pages)}
			</span>
			{said !== null && <span className="visually-hidden">{said}</span>}
		</span>
	);
}
