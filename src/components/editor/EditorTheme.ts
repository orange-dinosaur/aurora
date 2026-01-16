/**
 * Lexical Editor Theme Configuration
 * Defines CSS class names for editor elements
 */

import type { EditorThemeClasses } from 'lexical';

export const editorTheme: EditorThemeClasses = {
    paragraph: 'editor-paragraph',
    heading: {
        h1: 'editor-heading-h1',
        h2: 'editor-heading-h2',
        h3: 'editor-heading-h3',
        h4: 'editor-heading-h4',
        h5: 'editor-heading-h5',
        h6: 'editor-heading-h6',
    },
    text: {
        bold: 'editor-text-bold',
        italic: 'editor-text-italic',
        underline: 'editor-text-underline',
        strikethrough: 'editor-text-strikethrough',
        code: 'editor-text-code',
    },
    quote: 'editor-quote',
    list: {
        ul: 'editor-list-ul',
        ol: 'editor-list-ol',
        listitem: 'editor-listitem',
        nested: {
            listitem: 'editor-nested-listitem',
        },
    },
    mark: 'editor-mark',
};
