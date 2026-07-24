import { useState } from 'react';
import { act, fireEvent, render, waitFor } from '@testing-library/react-native';
import { SafeAreaProvider } from 'react-native-safe-area-context';

import { ThemeProvider } from '@/src/theme';
import {
  type LlmModelSettings,
  type LlmProvider,
} from '@/src/api/settingsStore';
import {
  LlmModelEditor,
  formatCost,
  type ModelPricing,
} from '@/src/components/settings/LlmModelEditor';

const baseProvider: LlmProvider = {
  id: 'llm-1',
  name: 'Test Provider',
  baseUrl: 'https://api.example.com/v1',
};

const baseModel: LlmModelSettings = {
  id: 'llm-model-1',
  name: 'Test Model',
  providerId: baseProvider.id,
  selectedModel: '',
  cachedModels: [],
};

const fetchMock = jest.fn();

beforeEach(() => {
  fetchMock.mockClear();
  (globalThis as any).fetch = fetchMock;
});

const safeAreaMetrics = {
  insets: { top: 0, left: 0, right: 0, bottom: 0 },
  frame: { x: 0, y: 0, width: 0, height: 0 },
};

function TestWrapper({
  initialModel,
  initialProvider,
  providers,
  apiKey,
  onChangeModel,
}: {
  initialModel: LlmModelSettings;
  initialProvider: LlmProvider;
  providers?: LlmProvider[];
  apiKey: string;
  onChangeModel?: (model: LlmModelSettings) => void;
}) {
  const [model, setModel] = useState(initialModel);
  const allProviders = providers ?? [initialProvider];

  return (
    <SafeAreaProvider initialMetrics={safeAreaMetrics}>
      <ThemeProvider>
        <LlmModelEditor
          model={model}
          providers={allProviders}
          provider={initialProvider}
          apiKey={apiKey}
          onChangeModel={(next) => {
            setModel(next);
            onChangeModel?.(next);
          }}
          onSave={jest.fn()}
          onCancel={jest.fn()}
        />
      </ThemeProvider>
    </SafeAreaProvider>
  );
}

async function setup(
  overrides: Partial<{
    model: Partial<LlmModelSettings>;
    provider: Partial<LlmProvider>;
    apiKey: string;
  }> = {},
) {
  const onChangeModel = jest.fn();

  const provider: LlmProvider = {
    ...baseProvider,
    ...overrides.provider,
  };

  const model: LlmModelSettings = {
    ...baseModel,
    providerId: provider.id,
    ...overrides.model,
  };

  const utils = await render(
    <TestWrapper
      initialModel={model}
      initialProvider={provider}
      apiKey={overrides.apiKey ?? 'sk-test'}
      onChangeModel={onChangeModel}
    />,
  );

  return {
    ...utils,
    onChangeModel,
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

describe('LlmModelEditor', () => {
  it('preserves existing cost when re-selecting the same model before fetching', async () => {
    const { getByText, onChangeModel } = await setup({
      model: {
        cachedModels: ['existing-model'],
        selectedModel: 'existing-model',
        cost: '$5 / 1M tokens',
      },
    });

    fireEvent.press(getByText('existing-model'));

    await waitFor(() =>
      expect(onChangeModel).toHaveBeenCalledWith(
        expect.objectContaining({
          selectedModel: 'existing-model',
          cost: '$5 / 1M tokens',
        }),
      ),
    );
  });

  it('preserves existing cost when the model id is edited to the same value', async () => {
    const { getByPlaceholderText, onChangeModel } = await setup({
      model: {
        selectedModel: 'existing-model',
        cost: '$5 / 1M tokens',
      },
    });

    fireEvent.changeText(
      getByPlaceholderText('モデルID（手入力可）'),
      'existing-model',
    );

    await waitFor(() =>
      expect(onChangeModel).toHaveBeenCalledWith(
        expect.objectContaining({
          selectedModel: 'existing-model',
          cost: '$5 / 1M tokens',
        }),
      ),
    );
  });

  it('clears cost when changing to a model without known pricing', async () => {
    const { getByPlaceholderText, onChangeModel } = await setup({
      model: {
        selectedModel: 'existing-model',
        cost: '$5 / 1M tokens',
      },
    });

    fireEvent.changeText(
      getByPlaceholderText('モデルID（手入力可）'),
      'another-model',
    );

    await waitFor(() =>
      expect(onChangeModel).toHaveBeenCalledWith(
        expect.objectContaining({
          selectedModel: 'another-model',
          cost: undefined,
        }),
      ),
    );
  });

  it('toggles the model list visibility when the header is pressed', async () => {
    const { getByText, getByTestId, queryByTestId } = await setup({
      model: {
        cachedModels: ['existing-model'],
        selectedModel: 'existing-model',
        cost: '$5 / 1M tokens',
      },
    });

    expect(getByTestId('model-list-expanded')).toBeTruthy();

    fireEvent.press(getByText('モデル一覧'));
    await waitFor(() => {
      expect(queryByTestId('model-list-expanded')).toBeNull();
    });

    fireEvent.press(getByText('モデル一覧'));
    await waitFor(() => {
      expect(getByTestId('model-list-expanded')).toBeTruthy();
    });
  });

  it('shows the bottom fold button when the model list exceeds 3/5 of the screen height', async () => {
    const { getByText, getByTestId, queryByText } = await setup({
      model: {
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
      expect(getByText('畳む')).toBeTruthy();
    });

    await act(async () => {
      list.props.onLayout({
        nativeEvent: { layout: { width: 100, height: 0, x: 0, y: 0 } },
      });
    });

    await waitFor(() => {
      expect(queryByText('畳む')).toBeNull();
    });
  });

  it('fetches models, displays cost on cards, and updates model cost', async () => {
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

    const { getByText, onChangeModel } = await setup({
      provider: { baseUrl: 'https://openrouter.ai/api/v1' },
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

    expect(onChangeModel).toHaveBeenLastCalledWith(
      expect.objectContaining({
        cachedModels: ['model-1', 'model-2'],
        selectedModel: 'model-1',
        cost: 'in $2.5, out $10 / 1M tokens',
      }),
    );
  });

  it('clears cached models, fetch timestamp, and cost when switching provider', async () => {
    const otherProvider: LlmProvider = {
      id: 'llm-2',
      name: 'Other Provider',
      baseUrl: 'https://other.example.com/v1',
    };
    const onChangeModel = jest.fn();
    const { getByText } = await render(
      <SafeAreaProvider initialMetrics={safeAreaMetrics}>
        <ThemeProvider>
          <LlmModelEditor
            model={{
              ...baseModel,
              cachedModels: ['model-a'],
              modelsFetchedAt: '2026-01-01T00:00:00.000Z',
              cost: '$5 / 1M tokens',
            }}
            providers={[baseProvider, otherProvider]}
            provider={baseProvider}
            apiKey="sk-test"
            onChangeModel={onChangeModel}
            onSave={jest.fn()}
            onCancel={jest.fn()}
          />
        </ThemeProvider>
      </SafeAreaProvider>,
    );

    fireEvent.press(getByText(`Provider: ${baseProvider.name}`));
    await waitFor(() => {
      expect(getByText(otherProvider.name)).toBeTruthy();
    });
    fireEvent.press(getByText(otherProvider.name));

    await waitFor(() =>
      expect(onChangeModel).toHaveBeenCalledWith(
        expect.objectContaining({
          providerId: otherProvider.id,
          cachedModels: [],
          modelsFetchedAt: undefined,
          cost: undefined,
        }),
      ),
    );
  });
});
