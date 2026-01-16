/**
 * Word Count Plugin for Lexical Editor
 * Requirements: 6.1, 6.2, 6.3
 */

import { useEffect } from 'react';
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext';
import { $getRoot } from 'lexical';
import type { SerializedEditorState } from '@/types';
import { useEditorStore } from '@/store/editorStore';

interface WordCountPluginProps {
    onChange?: (content: SerializedEditorState) => void;
}

/**
 * Counts words in a text string
 * Words are defined as sequences of non-whitespace characters
 */
export function countWords(text: string): number {
    const trimmed = text.trim();
    if (trimmed === '') return 0;
    return trimmed.split(/\s+/).length;
}

/**
 * Counts paragraphs in text
 * Paragraphs are separated by double newlines or are distinct block elements
 */
export function countParagraphs(text: string): number {
    const trimmed = text.trim();
    if (trimmed === '') return 0;
    // Split by double newlines or count non-empty lines
    const paragraphs = trimmed.split(/\n\s*\n/).filter((p) => p.trim() !== '');
    return Math.max(paragraphs.length, 1);
}

export function WordCountPlugin({ onChange }: WordCountPluginProps) {
    const [editor] = useLexicalComposerContext();
    const setWordCount = useEditorStore((state) => state.setWordCount);
    const setCharacterCount = useEditorStore(
        (state) => state.setCharacterCount
    );
    const setParagraphCount = useEditorStore(
        (state) => state.setParagraphCount
    );

    useEffect(() => {
        return editor.registerUpdateListener(({ editorState }) => {
            editorState.read(() => {
                const root = $getRoot();
                const textContent = root.getTextContent();

                // Calculate counts
                const words = countWords(textContent);
                const characters = textContent.length;
                const paragraphs = root.getChildrenSize();

                // Update store
                setWordCount(words);
                setCharacterCount(characters);
                setParagraphCount(paragraphs);

                // Notify parent of content change
                if (onChange) {
                    const serialized = editorState.toJSON();
                    onChange(serialized);
                }
            });
        });
    }, [editor, onChange, setWordCount, setCharacterCount, setParagraphCount]);

    return null;
}

export default WordCountPlugin;
