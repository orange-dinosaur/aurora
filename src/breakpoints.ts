// The widths where the window's layout changes. The stylesheets' window queries
// use the same two numbers and have to be changed with these. The page inside a
// side (editor, Book details, overview) measures 820 and 620 against the side
// instead, in container queries of its own.

/** Where the binder floats over the writing instead of sitting beside it. */
export const NARROW = "(max-width: 820px)";

/** Where the right panel floats over the page instead of taking width from it. */
export const OVERLAY = "(max-width: 1080px)";
