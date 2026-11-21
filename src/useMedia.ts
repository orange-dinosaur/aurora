import { useEffect, useState } from "react";

/** Whether a media query holds, kept current as the window or a setting changes. */
export function useMedia(query: string): boolean {
	const [matches, setMatches] = useState(
		() => window.matchMedia(query).matches,
	);

	useEffect(() => {
		const list = window.matchMedia(query);
		function changed(event: MediaQueryListEvent) {
			setMatches(event.matches);
		}

		setMatches(list.matches);
		list.addEventListener("change", changed);
		return () => list.removeEventListener("change", changed);
	}, [query]);

	return matches;
}
