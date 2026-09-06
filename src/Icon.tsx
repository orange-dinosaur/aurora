import type { ReactNode } from "react";

export type IconName =
	| "search"
	| "list"
	| "contrast"
	| "arrows-vertical"
	| "chevron-down"
	| "chevron-left"
	| "chevron-right"
	| "arrow-left-right"
	| "panel-left"
	| "panel-right"
	| "folder"
	| "book"
	| "link"
	| "plus"
	| "refresh"
	| "tag"
	| "trash"
	| "log-out"
	| "user"
	| "x"
	| "sun"
	| "moon"
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
	"chevron-left": <path d="m15 18-6-6 6-6" />,
	"chevron-right": <path d="m9 18 6-6-6-6" />,
	"arrow-left-right": (
		<>
			<path d="m8 3-4 4 4 4" />
			<path d="M4 7h16" />
			<path d="m16 21 4-4-4-4" />
			<path d="M20 17H4" />
		</>
	),
	link: (
		<>
			<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
			<path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
		</>
	),
	folder: (
		<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />
	),
	book: (
		<path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H19a1 1 0 0 1 1 1v18a1 1 0 0 1-1 1H6.5a1 1 0 0 1 0-5H20" />
	),
	"panel-left": (
		<>
			<rect x="3" y="3" width="18" height="18" rx="2" />
			<path d="M9 3v18" />
		</>
	),
	"panel-right": (
		<>
			<rect x="3" y="3" width="18" height="18" rx="2" />
			<path d="M15 3v18" />
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
	// The hole in the label is a dot rather than a ring: at 16px an outlined
	// circle of this size fills in anyway.
	tag: (
		<>
			<path d="M12.586 2.586A2 2 0 0 0 11.172 2H4a2 2 0 0 0-2 2v7.172a2 2 0 0 0 .586 1.414l8.704 8.704a2.426 2.426 0 0 0 3.42 0l6.58-6.58a2.426 2.426 0 0 0 0-3.42z" />
			<circle cx="7.5" cy="7.5" r=".5" fill="currentColor" />
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
	user: (
		<>
			<path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" />
			<circle cx="12" cy="7" r="4" />
		</>
	),
	x: <path d="M18 6 6 18M6 6l12 12" />,
	// The theme button wears whichever of these it is about to bring on. The
	// mock draws the half-filled circle for it, but that shape is already the
	// editor's focus mode here, and one glyph cannot mean two things in one
	// window.
	sun: (
		<>
			<circle cx="12" cy="12" r="4" />
			<path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41" />
		</>
	),
	moon: <path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z" />,
	"more-vertical": (
		<>
			<circle cx="12" cy="5" r="1" />
			<circle cx="12" cy="12" r="1" />
			<circle cx="12" cy="19" r="1" />
		</>
	),
};

/** Decorative by definition: every button carrying one keeps the label that
 * names it, so the icon has nothing left to say to a screen reader. The size
 * here is the one the interface uses; a caller wanting another passes a class
 * and sets it there. */
export default function Icon({
	name,
	className,
}: {
	name: IconName;
	className?: string;
}) {
	return (
		<svg
			className={className === undefined ? "icon" : `icon ${className}`}
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
