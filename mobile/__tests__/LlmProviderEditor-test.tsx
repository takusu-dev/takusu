import { useState } from 'react';
import { act, fireEvent, render, waitFor } from '@testing-library/react-native';

import { ThemeProvider } from '@/src/theme';
import { type LlmProviderSettings } from '@/src/api/settingsStore';
import {
  LlmProviderEditor,
  formatCost,
  type ModelPricing,
} from '@/src/components/settings/LlmProviderEditor';

const baseProvider: LlmProviderSettings = {
  id: 'llm-1',
  name: 'Test Provider',
  baseUrl: 'https://api.example.com/v1',
  selectedModel: '',
  cachedModels: [],
};

const fetchMock = jest.fn();

beforeEach(() => {
  fetchMock.mockClear();
  (globalThis as any).fetch = fetchMock;
});

function TestWrapper({
  initialProvider,
  apiKey,
  onChangeProvider,
}: {
  initialProvider: LlmProviderSettings;
  apiKey: string;
  onChangeProvider?: (provider: LlmProviderSettings) => void;
}) {
  const [provider, setProvider] = useState(initialProvider);

  return (
    <ThemeProvider>
      <LlmProviderEditor
        provider={provider}
        apiKey={apiKey}
        onChangeProvider={(next) => {
          setProvider(next);
          onChangeProvider?.(next);
        }}
        onChangeApiKey={jest.fn()}
        onSave={jest.fn()}
        onCancel={jest.fn()}
      />
    </ThemeProvider>
  );
}

async function setup(
  overrides: Partial<{
    provider: Partial<LlmProviderSettings>;
    apiKey: string;
  }> = {},
) {
  const onChangeProvider = jest.fn();

  const provider: LlmProviderSettings = {
    ...baseProvider,
    ...overrides.provider,
  };

  const utils = await render(
    <TestWrapper
      initialProvider={provider}
      apiKey={overrides.apiKey ?? 'sk-test'}
      onChangeProvider={onChangeProvider}
    />,
  );

  return {
    ...utils,
    onChangeProvider,
  };
}

describe('formatCost', () => {
  it('returns undefined when pricing is missing or empty', () => {
    expect(formatCost(undefined)).toBeUndefined();
    expect(formatCost({})).toBeUndefined();
    expect(formatCost({ prompt: '', completion: '' })).toBeUndefined();
  });

  it('formats prompt and completion per 1M tokens', () => {
    const pricing: ModelPricing = {
      prompt: '0.0000025',
      completion: '0.00001',
    };
    expect(formatCost(pricing)).toBe('in $2.5, out $10 / 1M tokens');
  });

  it('formats numeric pricing values', () => {
    expect(formatCost({ prompt: 0.000005, completion: 0.000015 })).toBe(
      'in $5, out $15 / 1M tokens',
    );
  });

  it('formats only prompt or only completion', () => {
    expect(formatCost({ prompt: '0.000005' })).toBe('$5 / 1M tokens');
    expect(formatCost({ completion: '0.000015' })).toBe('$15 / 1M tokens');
  });

  it('ignores invalid pricing values', () => {
    expect(formatCost({ prompt: 'not a number' })).toBeUndefined();
    expect(formatCost({ prompt: -1 })).toBeUndefined();
    expect(formatCost({ prompt: -1, completion: '0.00001' })).toBe(
      '$10 / 1M tokens',
    );
  });
});

describe('LlmProviderEditor', () => {
  it('preserves existing cost when re-selecting the same model before fetching', async () => {
    const { getByText, onChangeProvider } = await setup({
      provider: {
        ...baseProvider,
        cachedModels: ['existing-model'],
        selectedModel: 'existing-model',
        cost: '$5 / 1M tokens',
      },
    });

    fireEvent.press(getByText('● existing-model'));

    await waitFor(() =>
      expect(onChangeProvider).toHaveBeenCalledWith(
        expect.objectContaining({
          selectedModel: 'existing-model',
          cost: '$5 / 1M tokens',
        }),
      ),
    );
  });

  it('preserves existing cost when the model id is edited to the same value', async () => {
    const { getByPlaceholderText, onChangeProvider } = await setup({
      provider: {
        ...baseProvider,
        selectedModel: 'existing-model',
        cost: '$5 / 1M tokens',
      },
    });

    fireEvent.changeText(
      getByPlaceholderText('モデルID（手入力可）'),
      'existing-model',
    );

    await waitFor(() =>
      expect(onChangeProvider).toHaveBeenCalledWith(
        expect.objectContaining({
          selectedModel: 'existing-model',
          cost: '$5 / 1M tokens',
        }),
      ),
    );
  });

  it('clears cost when changing to a model without known pricing', async () => {
    const { getByPlaceholderText, onChangeProvider } = await setup({
      provider: {
        ...baseProvider,
        selectedModel: 'existing-model',
        cost: '$5 / 1M tokens',
      },
    });

    fireEvent.changeText(
      getByPlaceholderText('モデルID（手入力可）'),
      'another-model',
    );

    await waitFor(() =>
      expect(onChangeProvider).toHaveBeenCalledWith(
        expect.objectContaining({
          selectedModel: 'another-model',
          cost: undefined,
        }),
      ),
    );
  });

  it('toggles the model list visibility when the header is pressed', async () => {
    const { getByText, queryByText } = await setup({
      provider: {
        ...baseProvider,
        cachedModels: ['existing-model'],
        selectedModel: 'existing-model',
        cost: '$5 / 1M tokens',
      },
    });

    expect(getByText('● existing-model')).toBeTruthy();

    fireEvent.press(getByText('モデル一覧'));
    await waitFor(() => {
      expect(queryByText('● existing-model')).toBeNull();
    });

    fireEvent.press(getByText('モデル一覧'));
    await waitFor(() => {
      expect(getByText('● existing-model')).toBeTruthy();
    });
  });

  it('shows the bottom fold button when the model list exceeds 3/5 of the screen height', async () => {
    const { getByText, getByTestId, queryByText } = await setup({
      provider: {
        ...baseProvider,
        cachedModels: ['model-a', 'model-b', 'model-c'],
        selectedModel: 'model-a',
      },
    });

    const list = getByTestId('model-list-expanded');
    await act(async () => {
      list.props.onLayout({
        nativeEvent: { layout: { width: 100, height: 10000, x: 0, y: 0 } },
      });
    });

    await waitFor(() => {
      expect(getByText('▲ 畳む')).toBeTruthy();
    });

    await act(async () => {
      list.props.onLayout({
        nativeEvent: { layout: { width: 100, height: 0, x: 0, y: 0 } },
      });
    });

    await waitFor(() => {
      expect(queryByText('▲ 畳む')).toBeNull();
    });
  });

  it('fetches models, displays cost on cards, and updates provider cost', async () => {
    fetchMock.mockResolvedValueOnce({
      ok: true,
      json: jest.fn().mockResolvedValueOnce({
        data: [
          {
            id: 'model-1',
            pricing: { prompt: '0.0000025', completion: '0.00001' },
          },
          { id: 'model-2' },
        ],
      }),
    });

    const { getByText, onChangeProvider } = await setup({
      provider: { ...baseProvider, baseUrl: 'https://openrouter.ai/api/v1' },
    });

    fireEvent.press(getByText('モデルを取得'));

    await waitFor(() => {
      expect(getByText('in $2.5, out $10 / 1M tokens')).toBeTruthy();
    });

    expect(fetchMock).toHaveBeenCalledWith(
      'https://openrouter.ai/api/v1/models',
      expect.objectContaining({
        headers: { Authorization: 'Bearer sk-test' },
      }),
    );

    expect(onChangeProvider).toHaveBeenLastCalledWith(
      expect.objectContaining({
        cachedModels: ['model-1', 'model-2'],
        selectedModel: 'model-1',
        cost: 'in $2.5, out $10 / 1M tokens',
      }),
    );
  });
});
