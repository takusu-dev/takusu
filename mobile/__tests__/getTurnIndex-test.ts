import { getTurnIndex } from '@/src/utils/getTurnIndex';
import type { Message } from '@/src/api/agentSessionStore';

function msg(id: string, role: 'user' | 'assistant', text: string): Message {
  return {
    id,
    role,
    text,
    thinking: '',
    toolCalls: [],
    state: role === 'assistant' ? 'done' : undefined,
    segments: [],
    collapsedGroups: [],
  };
}

describe('getTurnIndex', () => {
  it('returns 0 for the first user message', () => {
    const messages: Message[] = [msg('u1', 'user', 'hello')];
    expect(getTurnIndex(messages, 0)).toBe(0);
  });

  it('returns 1 for the second user message', () => {
    const messages: Message[] = [
      msg('u1', 'user', 'hello'),
      msg('a1', 'assistant', 'hi'),
      msg('u2', 'user', 'again'),
    ];
    expect(getTurnIndex(messages, 2)).toBe(1);
  });

  it('returns the previous user turn index for an assistant message', () => {
    const messages: Message[] = [
      msg('u1', 'user', 'hello'),
      msg('a1', 'assistant', 'hi'),
      msg('u2', 'user', 'again'),
      msg('a2', 'assistant', 'sure'),
    ];
    expect(getTurnIndex(messages, 1)).toBe(0);
    expect(getTurnIndex(messages, 3)).toBe(1);
  });
});
