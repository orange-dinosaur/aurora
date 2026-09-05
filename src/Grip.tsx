import { useRef, type KeyboardEvent, type RefObject } from "react";
import { dragged, type Bounds } from "./panels";

type Props = {
	/** Which side of the writing the panel sits on. Both handles are on the
	 * inner edge, so a drag means opposite things on the two sides. */
	side: "left" | "right";
	/**
	 * The panel being resized. The drag moves it directly rather than through
	 * state: a width in React would re-render the project, the document list
	 * and the editor on every frame of every drag.
	 */
	panel: RefObject<HTMLElement | null>;
	/** The width it is at now, which is where a drag starts from. */
	width: number;
	bounds: Bounds;
	label: string;
	/** The width the drag settled on. Called once, on letting go, because this
	 * is a preference and every call writes the store. */
	onWidth: (width: number) => void;
};

/** How far an arrow key moves the edge. Wide enough to get somewhere, narrow
 * enough to arrive at a particular width. */
const STEP = 16;

/** The draggable edge of a panel. */
export default function Grip({
	side,
	panel,
	width,
	bounds,
	label,
	onWidth,
}: Props) {
	// Where the drag has reached, so letting go hands over what the moves
	// agreed on rather than the width this render was built with.
	const reached = useRef(width);

	function grab(grabbed: number) {
		const from = width;
		reached.current = from;

		function move(event: MouseEvent) {
			reached.current = dragged(
				side,
				from,
				event.clientX - grabbed,
				bounds,
			);
			panel.current?.style.setProperty("width", `${reached.current}px`);
		}

		function drop() {
			document.removeEventListener("mousemove", move);
			document.removeEventListener("mouseup", drop);
			document.body.classList.remove("dragging");
			onWidth(reached.current);
		}

		// The class carries the cursor and stops the writing being selected
		// under a pointer that is doing something else.
		document.body.classList.add("dragging");
		document.addEventListener("mousemove", move);
		document.addEventListener("mouseup", drop);
	}

	function nudge(event: KeyboardEvent) {
		if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") {
			return;
		}

		event.preventDefault();
		onWidth(
			dragged(
				side,
				width,
				event.key === "ArrowLeft" ? -STEP : STEP,
				bounds,
			),
		);
	}

	return (
		<div
			className={`grip grip--${side}`}
			role="separator"
			aria-orientation="vertical"
			aria-label={label}
			title={`Drag to resize · minimum ${bounds.min}px`}
			aria-valuenow={width}
			aria-valuemin={bounds.min}
			aria-valuemax={bounds.max}
			tabIndex={0}
			// Taken here rather than in the drag, so the press cannot start a
			// selection in the text on its way past.
			onMouseDown={(event) => {
				event.preventDefault();
				grab(event.clientX);
			}}
			onKeyDown={nudge}
		/>
	);
}
