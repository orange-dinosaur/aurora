/**
 * Core type definitions for Aurora - Novel Text Editor
 * Requirements: 1.1, 2.1-2.7, 3.1-3.5
 */

import type { SerializedEditorState as LexicalSerializedEditorState } from 'lexical';

// Re-export Lexical's SerializedEditorState for document content
export type SerializedEditorState = LexicalSerializedEditorState;

// Project Types

export interface CustomTheme {
    backgroundColor: string;
    textColor: string;
    accentColor: string;
    fontFamily: string;
    fontSize: number;
}

export interface ProjectSettings {
    dailyGoal: number;
    theme: 'light' | 'dark' | 'sepia' | 'custom';
    customTheme?: CustomTheme;
    typewriterScrolling: boolean;
    autoSaveInterval: number; // milliseconds
}

export interface Project {
    id: string;
    name: string;
    createdAt: Date;
    updatedAt: Date;
    settings: ProjectSettings;
}

// Document Types

export type DocumentType =
    | 'chapter'
    | 'scene'
    | 'note'
    | 'world-building'
    | 'character';

export type RevisionStatus = 'draft' | 'revised' | 'final';

export interface TimelineMarker {
    date?: string; // In-story date
    order: number; // Relative ordering
    description?: string;
}

export interface Document {
    id: string;
    projectId: string;
    type: DocumentType;
    title: string;
    content: SerializedEditorState;
    parentId?: string; // For scenes (parent is chapter)
    order: number;
    revisionStatus: RevisionStatus;
    wordCount: number;
    createdAt: Date;
    updatedAt: Date;
    timelinePosition?: TimelineMarker;
}

// Character Types

export interface Character {
    id: string;
    projectId: string;
    name: string;
    description: string;
    customFields: Record<string, string>;
    imageUrl?: string;
    createdAt: Date;
    updatedAt: Date;
}

export interface CharacterRelationship {
    id: string;
    projectId: string;
    character1Id: string;
    character2Id: string;
    relationshipType: string;
    description?: string;
}

export interface CharacterAppearance {
    characterId: string;
    documentId: string;
    mentions: number;
}

// Writing Statistics Types

export interface WritingSession {
    id: string;
    projectId: string;
    date: string; // ISO date
    wordsWritten: number;
    duration: number; // milliseconds
    documentId?: string;
}

export interface WritingSprint {
    id: string;
    projectId: string;
    startTime: Date;
    duration: number; // planned duration in milliseconds
    actualDuration?: number;
    wordsWritten: number;
    completed: boolean;
}

export interface DailyGoalProgress {
    date: string;
    goal: number;
    achieved: number;
    completed: boolean;
}

// Sync and Version Control Types

export type SyncStatus = 'synced' | 'syncing' | 'offline' | 'conflict';

export interface SyncConflict {
    documentId: string;
    localVersion: SerializedEditorState;
    remoteVersion: SerializedEditorState;
    detectedAt: Date;
}

export interface SyncState {
    status: SyncStatus;
    lastSyncedAt?: Date;
    pendingChanges: number;
    conflicts: SyncConflict[];
}

export interface GitCommit {
    hash: string;
    message: string;
    author: string;
    date: Date;
    files: string[];
}

export interface Snapshot {
    id: string;
    projectId: string;
    createdAt: Date;
    reason: 'manual' | 'auto-bulk-replace' | 'auto-delete';
    description?: string;
    filePath: string;
}
