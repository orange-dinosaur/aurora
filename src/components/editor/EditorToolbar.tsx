/**
 * Editor Toolbar Component
 * Requirements: 1.2, 1.7
 */

import { useCallback, useEffect, useState } from 'react';
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext';
import {
    $getSelection,
    $isRangeSelection,
    $createParagraphNode,
    FORMAT_TEXT_COMMAND,
    COMMAND_PRIORITY_CRITICAL,
    KEY_MODIFIER_COMMAND,
} from 'lexical';
import { $setBlocksType } from '@lexical/selection';
import {
    $createHeadingNode,
    $createQuoteNode,
    HeadingTagType,
} from '@lexical/rich-text';
import {
    INSERT_ORDERED_LIST_COMMAND,
    INSERT_UNORDERED_LIST_COMMAND,
    REMOVE_LIST_COMMAND,
    ListNode,
} from '@lexical/list';
import { $getNearestNodeOfType } from '@lexical/utils';
import './EditorToolbar.css';

type BlockType = 'paragraph' | 'h1' | 'h2' | 'h3' | 'quote' | 'ul' | 'ol';

export function EditorToolbar() {
    const [editor] = useLexicalComposerContext();
    const [isBold, setIsBold] = useState(false);
    const [isItalic, setIsItalic] = useState(false);
    const [isUnderline, setIsUnderline] = useState(false);
    const [blockType, setBlockType] = useState<BlockType>('paragraph');

    // Update toolbar state based on selection
    const updateToolbar = useCallback(() => {
        const selection = $getSelection();
        if ($isRangeSelection(selection)) {
            setIsBold(selection.hasFormat('bold'));
            setIsItalic(selection.hasFormat('italic'));
            setIsUnderline(selection.hasFormat('underline'));

            const anchorNode = selection.anchor.getNode();
            const element =
                anchorNode.getKey() === 'root'
                    ? anchorNode
                    : anchorNode.getTopLevelElementOrThrow();

            const elementKey = element.getKey();
            const elementDOM = editor.getElementByKey(elementKey);

            if (elementDOM !== null) {
                const listNode = $getNearestNodeOfType(anchorNode, ListNode);
                if (listNode) {
                    const listType = listNode.getListType();
                    setBlockType(listType === 'number' ? 'ol' : 'ul');
                } else {
                    const type = element.getType();
                    if (type === 'heading') {
                        const tag = (
                            element as unknown as {
                                getTag: () => HeadingTagType;
                            }
                        ).getTag();
                        setBlockType(tag as BlockType);
                    } else if (type === 'quote') {
                        setBlockType('quote');
                    } else {
                        setBlockType('paragraph');
                    }
                }
            }
        }
    }, [editor]);

    useEffect(() => {
        return editor.registerUpdateListener(({ editorState }) => {
            editorState.read(() => {
                updateToolbar();
            });
        });
    }, [editor, updateToolbar]);

    // Register keyboard shortcuts
    useEffect(() => {
        return editor.registerCommand(
            KEY_MODIFIER_COMMAND,
            (payload) => {
                const event = payload as KeyboardEvent;
                const { code, ctrlKey, metaKey } = event;
                const isModifier = ctrlKey || metaKey;

                if (!isModifier) return false;

                if (code === 'KeyB') {
                    event.preventDefault();
                    editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'bold');
                    return true;
                }
                if (code === 'KeyI') {
                    event.preventDefault();
                    editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'italic');
                    return true;
                }
                if (code === 'KeyU') {
                    event.preventDefault();
                    editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'underline');
                    return true;
                }
                return false;
            },
            COMMAND_PRIORITY_CRITICAL
        );
    }, [editor]);

    const formatBold = () => {
        editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'bold');
    };

    const formatItalic = () => {
        editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'italic');
    };

    const formatUnderline = () => {
        editor.dispatchCommand(FORMAT_TEXT_COMMAND, 'underline');
    };

    const formatHeading = (headingTag: HeadingTagType) => {
        editor.update(() => {
            const selection = $getSelection();
            if ($isRangeSelection(selection)) {
                $setBlocksType(selection, () => $createHeadingNode(headingTag));
            }
        });
    };

    const formatQuote = () => {
        editor.update(() => {
            const selection = $getSelection();
            if ($isRangeSelection(selection)) {
                if (blockType === 'quote') {
                    // Toggle off - convert back to paragraph
                    $setBlocksType(selection, () => $createParagraphNode());
                } else {
                    $setBlocksType(selection, () => $createQuoteNode());
                }
            }
        });
    };

    const formatBulletList = () => {
        if (blockType === 'ul') {
            editor.dispatchCommand(REMOVE_LIST_COMMAND, undefined);
        } else {
            editor.dispatchCommand(INSERT_UNORDERED_LIST_COMMAND, undefined);
        }
    };

    const formatNumberedList = () => {
        if (blockType === 'ol') {
            editor.dispatchCommand(REMOVE_LIST_COMMAND, undefined);
        } else {
            editor.dispatchCommand(INSERT_ORDERED_LIST_COMMAND, undefined);
        }
    };

    const handleBlockTypeChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
        const value = e.target.value as BlockType;
        switch (value) {
            case 'h1':
            case 'h2':
            case 'h3':
                formatHeading(value);
                break;
            case 'quote':
                formatQuote();
                break;
            case 'ul':
                formatBulletList();
                break;
            case 'ol':
                formatNumberedList();
                break;
            default:
                // Reset to paragraph - handled by removing formatting
                break;
        }
    };

    return (
        <div
            className="editor-toolbar"
            role="toolbar"
            aria-label="Formatting options">
            <div className="toolbar-group">
                <select
                    className="toolbar-select"
                    value={blockType}
                    onChange={handleBlockTypeChange}
                    aria-label="Block type">
                    <option value="paragraph">Paragraph</option>
                    <option value="h1">Heading 1</option>
                    <option value="h2">Heading 2</option>
                    <option value="h3">Heading 3</option>
                </select>
            </div>

            <div className="toolbar-group">
                <button
                    type="button"
                    className={`toolbar-button ${isBold ? 'active' : ''}`}
                    onClick={formatBold}
                    aria-label="Bold (Ctrl+B)"
                    title="Bold (Ctrl+B)">
                    B
                </button>
                <button
                    type="button"
                    className={`toolbar-button ${isItalic ? 'active' : ''}`}
                    onClick={formatItalic}
                    aria-label="Italic (Ctrl+I)"
                    title="Italic (Ctrl+I)"
                    style={{ fontStyle: 'italic' }}>
                    I
                </button>
                <button
                    type="button"
                    className={`toolbar-button ${isUnderline ? 'active' : ''}`}
                    onClick={formatUnderline}
                    aria-label="Underline (Ctrl+U)"
                    title="Underline (Ctrl+U)"
                    style={{ textDecoration: 'underline' }}>
                    U
                </button>
            </div>

            <div className="toolbar-group">
                <button
                    type="button"
                    className={`toolbar-button ${blockType === 'ul' ? 'active' : ''}`}
                    onClick={formatBulletList}
                    aria-label="Bullet list"
                    title="Bullet list">
                    •
                </button>
                <button
                    type="button"
                    className={`toolbar-button ${blockType === 'ol' ? 'active' : ''}`}
                    onClick={formatNumberedList}
                    aria-label="Numbered list"
                    title="Numbered list">
                    1.
                </button>
                <button
                    type="button"
                    className={`toolbar-button ${blockType === 'quote' ? 'active' : ''}`}
                    onClick={formatQuote}
                    aria-label="Block quote"
                    title="Block quote">
                    "
                </button>
            </div>
        </div>
    );
}

export default EditorToolbar;
