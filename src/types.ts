// What the Rust commands send and take. ts-rs generates these from the structs
// into `src/bindings/` whenever `cargo test` runs; a few keep the name the
// frontend knew them by before that.

import type { NodeView as TreeNode } from "./bindings/NodeView";
import type { Session } from "./bindings/Session";

export type { Book } from "./bindings/Book";
export type { ChildSummary as OverviewCard } from "./bindings/ChildSummary";
export type { Contributor } from "./bindings/Contributor";
export type { DocumentSummary } from "./bindings/DocumentSummary";
export type { DocumentText } from "./bindings/DocumentText";
export type { DocumentView as ProjectDocument } from "./bindings/DocumentView";
export type { ExportFormat } from "./bindings/ExportFormat";
export type { Extent } from "./bindings/Extent";
export type { FolderKind } from "./bindings/FolderKind";
export type { FolderProgress } from "./bindings/FolderProgress";
export type { Format } from "./bindings/Format";
export type { FormatLayout } from "./bindings/FormatLayout";
export type { LastProject } from "./bindings/LastProject";
export type { ManuscriptFont } from "./bindings/ManuscriptFont";
export type { OpenedProject as OpenProject } from "./bindings/OpenedProject";
export type { Preferences } from "./bindings/Preferences";
export type { Progress as ExportProgress } from "./bindings/Progress";
export type { RecentProject } from "./bindings/RecentProject";
export type { RecentSummary } from "./bindings/RecentSummary";
export type { RightSidebarTab } from "./bindings/RightSidebarTab";
export type { Role } from "./bindings/Role";
export type { SprintUnit } from "./bindings/SprintUnit";
export type { Theme } from "./bindings/Theme";
export type { TrashEntry } from "./bindings/TrashEntry";
export type { TreeNode };
export type { Session as PastSession };

/** The folder half of a `TreeNode`, for the commands that only return one. */
export type FolderNode = Extract<TreeNode, { node: "folder" }>;

/**
 * A session as it is sent to be recorded. The id is missing on purpose: Rust
 * mints one for every session that arrives without it. The frontend always
 * says whether a limit was met and what each document gained and lost.
 */
export type SessionRecord = Omit<Session, "id"> &
	Required<Pick<Session, "limitMet" | "documents">>;
