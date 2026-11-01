import Controls from "./Controls";
import Icon from "./Icon";

type Props = {
	/** Set while the page shows marks, when there is no text to format. */
	marking: boolean;
	focus: boolean;
	typewriter: boolean;
	onFocus: () => void;
	onTypewriter: () => void;
};

// The dock at the foot of the page. It is always there, so what can be done to
// the text is visible without having to select anything first. The two ways of
// writing stay live while the page shows marks, because they are not about the
// words.
export default function Toolbar({
	marking,
	focus,
	typewriter,
	onFocus,
	onTypewriter,
}: Props) {
	return (
		<div className="toolbar">
			<div className="toolbar__marks" inert={marking}>
				<Controls />
			</div>
			<span className="controls__divider" aria-hidden="true" />
			{/* Pressing one must not take the caret out of the text. */}
			<div
				className="toolbar__modes"
				onMouseDown={(event) => event.preventDefault()}
			>
				<button
					type="button"
					className="toolbar__mode"
					aria-pressed={focus}
					title="Leave only the page, and dim other paragraphs"
					onClick={onFocus}
				>
					<Icon name="contrast" />
					Focus
				</button>
				<button
					type="button"
					className="toolbar__mode"
					aria-pressed={typewriter}
					aria-label="Typewriter scrolling"
					title="Hold the line being written on"
					onClick={onTypewriter}
				>
					<Icon name="arrows-vertical" />
				</button>
			</div>
		</div>
	);
}
