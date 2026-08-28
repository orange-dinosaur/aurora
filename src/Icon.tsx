import type { ReactNode } from "react";

export type IconName =
	| "search"
	| "list"
	| "contrast"
	| "arrows-vertical"
	| "chevron-down"
	| "chevron-right"
	| "panel-left"
	| "plus"
	| "refresh"
	| "trash"
	| "log-out"
	| "x"
	| "more-vertical";

// Lucide's geometry, on its 24-unit grid. Anything added here should be traced
// from the same set rather than drawn by eye, or the family stops reading as
// one hand.
const SHAPES: Record<IconName, ReactNode> = {
	search: (
		<>
			<circle cx="11" cy="11" r="8" />
			<path d="m21 21-4.3-4.3" />
		</>
	),
	list: (
		<>
			<path d="M3 6h.01M3 12h.01M3 18h.01" />
			<path d="M8 6h13M8 12h13M8 18h13" />
		</>
	),
	// The one shape drawn part solid: read as an outline it is a circle beside
	// a D, and says nothing about dimming.
	contrast: (
		<>
			<circle cx="12" cy="12" r="10" />
			<path d="M12 18a6 6 0 0 1 0-12v12z" fill="currentColor" />
		</>
	),
	"arrows-vertical": (
		<>
			<path d="M12 2v20" />
			<path d="m8 6 4-4 4 4" />
			<path d="m8 18 4 4 4-4" />
		</>
	),
	"chevron-down": <path d="m6 9 6 6 6-6" />,
	"chevron-right": <path d="m9 18 6-6-6-6" />,
	"panel-left": (
		<>
			<rect x="3" y="3" width="18" height="18" rx="2" />
			<path d="M9 3v18" />
		</>
	),
	plus: <path d="M5 12h14M12 5v14" />,
	refresh: (
		<>
			<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" />
			<path d="M21 3v5h-5" />
			<path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" />
			<path d="M8 16H3v5" />
		</>
	),
	trash: (
		<>
			<path d="M3 6h18" />
			<path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
			<path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
			<path d="M10 11v6M14 11v6" />
		</>
	),
	"log-out": (
		<>
			<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
			<path d="m16 17 5-5-5-5" />
			<path d="M21 12H9" />
		</>
	),
	x: <path d="M18 6 6 18M6 6l12 12" />,
	"more-vertical": (
		<>
			<circle cx="12" cy="5" r="1" />
			<circle cx="12" cy="12" r="1" />
			<circle cx="12" cy="19" r="1" />
		</>
	),
};

/** Decorative by definition: every button carrying one keeps the label that
 * names it, so the icon has nothing left to say to a screen reader. */
export default function Icon({ name }: { name: IconName }) {
	return (
		<svg
			className="icon"
			viewBox="0 0 24 24"
			width="16"
			height="16"
			fill="none"
			stroke="currentColor"
			/* In grid units, and the grid is drawn at two thirds size, so this
			   is the 1.4px the design asks for. */
			strokeWidth="2.1"
			strokeLinecap="round"
			strokeLinejoin="round"
			aria-hidden="true"
		>
			{SHAPES[name]}
		</svg>
	);
}
