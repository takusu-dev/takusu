import type { Message } from '@/src/api/agentSessionStore';

export function getTurnIndex(messages: Message[], index: number): number {
  const userCount = messages
    .slice(0, index)
    .filter((m) => m.role === 'user').length;
  if (messages[index]?.role === 'user') return userCount;
  return Math.max(0, userCount - 1);
}
