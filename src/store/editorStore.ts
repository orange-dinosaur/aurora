/**
 * Editor State Store using Zustand
 * Manages word count, character count, and paragraph count
 */

import { create } from 'zustand';

interface EditorState {
    wordCount: number;
    characterCount: number;
    paragraphCount: number;
    setWordCount: (count: number) => void;
    setCharacterCount: (count: number) => void;
    setParagraphCount: (count: number) => void;
}

export const useEditorStore = create<EditorState>((set) => ({
    wordCount: 0,
    characterCount: 0,
    paragraphCount: 0,
    setWordCount: (count) => set({ wordCount: count }),
    setCharacterCount: (count) => set({ characterCount: count }),
    setParagraphCount: (count) => set({ paragraphCount: count }),
}));
