jest.mock('@react-native-async-storage/async-storage', () => ({}));

import { parseTtsProviders } from '@/src/api/settingsStore';

describe('parseTtsProviders', () => {
  it('returns an empty array for null', () => {
    expect(parseTtsProviders(null)).toEqual([]);
  });

  it('returns an empty array for invalid JSON', () => {
    expect(parseTtsProviders('not json')).toEqual([]);
  });

  it('keeps valid cartesia providers', () => {
    const input = JSON.stringify([
      {
        id: 'p1',
        name: 'Cartesia',
        provider: 'cartesia',
        voiceId: 'voice-1',
        language: 'ja',
        sampleRate: 44100,
        speed: 1.2,
      },
    ]);
    expect(parseTtsProviders(input)).toEqual([
      {
        id: 'p1',
        name: 'Cartesia',
        provider: 'cartesia',
        voiceId: 'voice-1',
        language: 'ja',
        sampleRate: 44100,
        speed: 1.2,
      },
    ]);
  });

  it('keeps valid android providers', () => {
    const input = JSON.stringify([
      {
        id: 'p2',
        name: 'Android',
        provider: 'android',
        voiceId: '',
        language: 'ja',
        sampleRate: 44100,
      },
    ]);
    const result = parseTtsProviders(input);
    expect(result).toHaveLength(1);
    expect(result[0]?.provider).toBe('android');
  });

  it('falls back invalid provider names to cartesia', () => {
    const input = JSON.stringify([
      {
        id: 'p3',
        name: 'Bad',
        provider: 'unknown',
        voiceId: '',
        language: 'ja',
        sampleRate: 44100,
      },
    ]);
    const result = parseTtsProviders(input);
    expect(result).toHaveLength(1);
    expect(result[0]?.provider).toBe('cartesia');
  });

  it('fixes out-of-range speed values', () => {
    const input = JSON.stringify([
      {
        id: 'p4',
        name: 'Fast',
        provider: 'cartesia',
        voiceId: 'v',
        language: 'ja',
        sampleRate: 44100,
        speed: 0,
      },
      {
        id: 'p5',
        name: 'Slow',
        provider: 'cartesia',
        voiceId: 'v',
        language: 'ja',
        sampleRate: 44100,
        speed: -1,
      },
    ]);
    const result = parseTtsProviders(input);
    expect(result[0]?.speed).toBeUndefined();
    expect(result[1]?.speed).toBeUndefined();
  });

  it('fixes invalid sampleRate', () => {
    const input = JSON.stringify([
      {
        id: 'p6',
        name: 'Cartesia',
        provider: 'cartesia',
        voiceId: 'v',
        language: 'ja',
        sampleRate: -1,
      },
    ]);
    const result = parseTtsProviders(input);
    expect(result[0]?.sampleRate).toBe(44100);
  });

  it('skips entries without a valid id', () => {
    const input = JSON.stringify([
      { provider: 'android', voiceId: '', language: 'ja', sampleRate: 44100 },
    ]);
    expect(parseTtsProviders(input)).toEqual([]);
  });
});
