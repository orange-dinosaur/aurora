/**
 * Unit tests for Word Count Plugin
 * Requirements: 6.1, 6.2, 6.3
 */

import { describe, it, expect } from 'vitest';
import { countWords, countParagraphs } from './WordCountPlugin';

describe('countWords', () => {
    it('returns 0 for empty string', () => {
        expect(countWords('')).toBe(0);
    });

    it('returns 0 for whitespace only', () => {
        expect(countWords('   ')).toBe(0);
        expect(countWords('\n\t  ')).toBe(0);
    });

    it('counts single word', () => {
        expect(countWords('hello')).toBe(1);
    });

    it('counts multiple words', () => {
        expect(countWords('hello world')).toBe(2);
        expect(countWords('one two three four')).toBe(4);
    });

    it('handles multiple spaces between words', () => {
        expect(countWords('hello    world')).toBe(2);
    });

    it('handles newlines and tabs', () => {
        expect(countWords('hello\nworld')).toBe(2);
        expect(countWords('hello\tworld')).toBe(2);
    });

    it('trims leading and trailing whitespace', () => {
        expect(countWords('  hello world  ')).toBe(2);
    });
});

describe('countParagraphs', () => {
    it('returns 0 for empty string', () => {
        expect(countParagraphs('')).toBe(0);
    });

    it('returns 0 for whitespace only', () => {
        expect(countParagraphs('   ')).toBe(0);
    });

    it('counts single paragraph', () => {
        expect(countParagraphs('This is a paragraph.')).toBe(1);
    });

    it('counts multiple paragraphs separated by double newlines', () => {
        expect(countParagraphs('First paragraph.\n\nSecond paragraph.')).toBe(
            2
        );
    });

    it('handles multiple blank lines between paragraphs', () => {
        expect(countParagraphs('First.\n\n\n\nSecond.')).toBe(2);
    });
});
