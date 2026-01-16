/**
 * Base Lexical Editor Component
 * Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 1.6
 */

import { LexicalComposer } from '@lexical/react/LexicalComposer';
import { RichTextPlugin } from '@lexical/react/LexicalRichTextPlugin';
import { ContentEditable } from '@lexical/react/LexicalContentEditable';
import { HistoryPlugin } from '@lexical/react/LexicalHistoryPlugin';
import { ListPlugin } from '@lexical/react/LexicalListPlugin';
import { LexicalErrorBoundary } from '@lexical/react/LexicalErrorBoundary';
import { HeadingNode, QuoteNode } from '@lexical/rich-text';
import { ListNode, ListItemNode } from '@lexical/list';
import { MarkNode } from '@lexical/mark';
import type { SerializedEditorState } from '@/types';
import { editorTheme } from './EditorTheme';
import { EditorToolbar } from './EditorToolbar';
import { WordCountPlugin } from './plugins/WordCountPlugin';
import { StatusBar } from './StatusBar';
import './Editor.css';

interface EditorProps {
    initialContent?: SerializedEditorState;
    onChange?: (content: SerializedEditorState) => void;
    placeholder?: string;
}

// Core nodes for rich text editing
const editorNodes = [HeadingNode, QuoteNode, ListNode, ListItemNode, MarkNode];

function onError(error: Error): void {
    console.error('Lexical Editor Error:', error);
}

function Placeholder({ text }: { text: string }) {
    return <div className="editor-placeholder">{text}</div>;
}

export function Editor({
    initialContent,
    onChange,
    placeholder = 'Start writing your story...',
}: EditorProps) {
    const initialConfig = {
        namespace: 'AuroraEditor',
        theme: editorTheme,
        nodes: editorNodes,
        onError,
        editorState: initialContent
            ? JSON.stringify(initialContent)
            : undefined,
    };

    return (
        <LexicalComposer initialConfig={initialConfig}>
            <div className="editor-container">
                <EditorToolbar />
                <div className="editor-inner">
                    <RichTextPlugin
                        contentEditable={
                            <ContentEditable
                                className="editor-input"
                                aria-placeholder={placeholder}
                                placeholder={<Placeholder text={placeholder} />}
                            />
                        }
                        ErrorBoundary={LexicalErrorBoundary}
                    />
                    <HistoryPlugin />
                    <ListPlugin />
                    <WordCountPlugin onChange={onChange} />
                </div>
                <StatusBar />
            </div>
        </LexicalComposer>
    );
}

export default Editor;
