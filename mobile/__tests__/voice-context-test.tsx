import { Pressable, Text, View } from 'react-native';
import { render, fireEvent, waitFor } from '@testing-library/react-native';

import { VoiceProvider, useVoice } from '@/src/api/VoiceContext';

function TestProbe() {
  const { isRecording, setIsRecording, pendingSessionId, setPendingSessionId } =
    useVoice();

  return (
    <View>
      <Text testID="recording">{String(isRecording)}</Text>
      <Text testID="session">{pendingSessionId ?? 'null'}</Text>
      <Pressable testID="start-recording" onPress={() => setIsRecording(true)}>
        <Text>start recording</Text>
      </Pressable>
      <Pressable testID="stop-recording" onPress={() => setIsRecording(false)}>
        <Text>stop recording</Text>
      </Pressable>
      <Pressable
        testID="set-session"
        onPress={() => setPendingSessionId('sess-123')}
      >
        <Text>set session</Text>
      </Pressable>
      <Pressable
        testID="clear-session"
        onPress={() => setPendingSessionId(null)}
      >
        <Text>clear session</Text>
      </Pressable>
    </View>
  );
}

async function setup(
  onRecordingChange?: (
    listener: (recording: boolean) => void,
  ) => (() => void) | void,
) {
  return render(
    <VoiceProvider onRecordingChange={onRecordingChange}>
      <TestProbe />
    </VoiceProvider>,
  );
}

describe('VoiceProvider', () => {
  it('provides default values', async () => {
    const { getByTestId } = await setup();

    expect(getByTestId('recording').children).toContain('false');
    expect(getByTestId('session').children).toContain('null');
  });

  it('updates isRecording', async () => {
    const { getByTestId } = await setup();

    expect(getByTestId('recording').children).toContain('false');

    fireEvent.press(getByTestId('start-recording'));
    await waitFor(() =>
      expect(getByTestId('recording').children).toContain('true'),
    );

    fireEvent.press(getByTestId('stop-recording'));
    await waitFor(() =>
      expect(getByTestId('recording').children).toContain('false'),
    );
  });

  it('registers recording listener on mount', async () => {
    const onRecordingChange = jest.fn((_listener) => jest.fn());
    await setup(onRecordingChange);
    expect(onRecordingChange).toHaveBeenCalled();
  });

  it('updates pendingSessionId', async () => {
    const { getByTestId } = await setup();

    fireEvent.press(getByTestId('set-session'));
    await waitFor(() =>
      expect(getByTestId('session').children).toContain('sess-123'),
    );

    fireEvent.press(getByTestId('clear-session'));
    await waitFor(() =>
      expect(getByTestId('session').children).toContain('null'),
    );
  });
});
