import { MarkdownIt } from 'react-native-markdown-renderer';

const md = new MarkdownIt({ typographer: false });

interface Token {
  type: string;
  content?: string;
  children?: Token[];
}

function extractText(tokens: Token[]): string {
  const parts: string[] = [];

  for (const token of tokens) {
    const text = tokenText(token);
    if (text.length > 0) {
      parts.push(text);
    }
  }

  return parts.join(' ').replace(/\s+/g, ' ').trim();
}

function tokenText(token: Token): string {
  // Image content is alt text; its children duplicate the same text, so skip
  // the recursion for images.
  if (token.children && token.children.length > 0 && token.type !== 'image') {
    return extractText(token.children);
  }

  switch (token.type) {
    case 'text':
    case 'code_inline':
      return token.content ?? '';
    case 'image':
      return token.content ?? '';
    case 'softbreak':
    case 'hardbreak':
      return ' ';
    default:
      // Ignore containers, code blocks, HTML blocks/inline, thematic breaks, etc.
      return '';
  }
}

export function markdownToSpeech(text: string): string {
  if (text.trim().length === 0) {
    return '';
  }

  const tokens = md.parse(text, {}) as Token[];
  return extractText(tokens);
}
