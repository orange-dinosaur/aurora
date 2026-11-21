// The one button that changes the theme, wherever a screen has room for it.
// The project header carries it beside the other toggles; the screens with no
// header carry it in the bar across their top, which is the only one they have.

import Icon from "./Icon";
import { flipped } from "./theme";
import type { Theme } from "./types";
import { useMedia } from "./useMedia";

export default function ThemeToggle({
	theme,
	onTheme,
	className,
}: {
	theme: Theme;
	onTheme: (theme: Theme) => void;
	/** The shape the surrounding row gives its buttons. */
	className: string;
}) {
	// The preference has three states and this button has two, so it has to
	// know what the desktop is set to as well as what the writer asked for.
	// Watched rather than read once: a desktop that changes at dusk would
	// otherwise leave the button offering the theme already on screen.
	const systemIsDark = useMedia("(prefers-color-scheme: dark)");

	const next = flipped(theme, systemIsDark);

	return (
		<button
			type="button"
			className={className}
			aria-label={`Switch to the ${next} theme`}
			title={`Switch to the ${next} theme`}
			onClick={() => onTheme(next)}
		>
			{/* The theme it brings on rather than the one showing: a button
			    says what pressing it does. */}
			<Icon name={next === "dark" ? "moon" : "sun"} />
		</button>
	);
}
