/**
 * Status Bar Component
 * Displays word count, character count, and paragraph count
 * Requirements: 6.1, 6.2, 6.3
 */

import { useEditorStore } from '@/store/editorStore';
import './StatusBar.css';

/**
 * Estimates reading time based on word count
 * Average reading speed: 200-250 words per minute
 */
function estimateReadingTime(wordCount: number): string {
    const wordsPerMinute = 225;
    const minutes = Math.ceil(wordCount / wordsPerMinute);
    if (minutes < 1) return '< 1 min';
    if (minutes === 1) return '1 min';
    return `${minutes} mins`;
}

export function StatusBar() {
    const wordCount = useEditorStore((state) => state.wordCount);
    const characterCount = useEditorStore((state) => state.characterCount);
    const paragraphCount = useEditorStore((state) => state.paragraphCount);

    return (
        <div className="status-bar" role="status" aria-live="polite">
            <div className="status-bar-left">
                <div className="status-item">
                    <span className="status-label">Words:</span>
                    <span className="status-value">
                        {wordCount.toLocaleString()}
                    </span>
                </div>
                <div className="status-item">
                    <span className="status-label">Characters:</span>
                    <span className="status-value">
                        {characterCount.toLocaleString()}
                    </span>
                </div>
                <div className="status-item">
                    <span className="status-label">Paragraphs:</span>
                    <span className="status-value">
                        {paragraphCount.toLocaleString()}
                    </span>
                </div>
            </div>
            <div className="status-bar-right">
                <div className="status-item">
                    <span className="status-label">Reading time:</span>
                    <span className="status-value">
                        {estimateReadingTime(wordCount)}
                    </span>
                </div>
            </div>
        </div>
    );
}

export default StatusBar;
