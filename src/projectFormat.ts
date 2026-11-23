import { createContext, useContext } from "react";
import type { Format } from "./types";

/**
 * The open project's format. It is fixed when the project is made, so it never
 * changes while the project is open, and anything in the binder may read it
 * without it being handed down row by row.
 */
export const FormatContext = createContext<Format>("novel");

export function useFormat(): Format {
	return useContext(FormatContext);
}
