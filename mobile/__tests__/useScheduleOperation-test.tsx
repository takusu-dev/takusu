import { renderHook, act, waitFor } from '@testing-library/react-native';
import { AppState } from 'react-native';

import TakusuServerModule from '@/modules/takusu-server/src/TakusuServerModule';
import { useScheduleOperation } from '@/src/hooks/useScheduleOperation';

jest.mock('@/modules/takusu-server/src/TakusuServerModule', () => ({
  status: jest.fn(),
  runScheduleOperation: jest.fn(),
  getScheduleOperationStatus: jest.fn(),
  clearScheduleOperationStatus: jest.fn(),
}));

describe('useScheduleOperation', () => {
  const mockClient = { triggerSync: jest.fn().mockResolvedValue(undefined) };
  const mockRefresh = jest.fn().mockResolvedValue(undefined);
  const mockShowTopToast = jest.fn().mockReturnValue('toast-1');
  const mockHideTopToast = jest.fn();
  const mockAppStateRemove = jest.fn();
  let appStateHandlers: Array<(state: string) => void> = [];

  const mockedTakusuServerModule = TakusuServerModule as unknown as {
    status: jest.Mock;
    runScheduleOperation: jest.Mock;
    getScheduleOperationStatus: jest.Mock;
    clearScheduleOperationStatus: jest.Mock;
  };

  const port = 4242;

  async function renderUseScheduleOperation() {
    return renderHook(() =>
      useScheduleOperation({
        client: mockClient,
        workersUrl: 'https://example.com',
        workersToken: 'token',
        refresh: mockRefresh,
        showTopToast: mockShowTopToast,
        hideTopToast: mockHideTopToast,
      }),
    );
  }

  beforeEach(() => {
    jest.useFakeTimers();
    jest.clearAllMocks();
    mockShowTopToast.mockReturnValue('toast-1');
    appStateHandlers = [];

    (AppState as any).addEventListener = jest.fn(
      (_event: string, handler: (state: string) => void) => {
        appStateHandlers.push(handler);
        return { remove: mockAppStateRemove };
      },
    );
    mockedTakusuServerModule.getScheduleOperationStatus.mockResolvedValue({
      status: 'running',
    });
    mockedTakusuServerModule.status.mockReturnValue({
      running: true,
      port,
    });
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  it('enqueues a generate operation and refreshes after success, then triggers GCal sync', async () => {
    const { result } = await renderUseScheduleOperation();

    await act(() => {
      result.current.startScheduleOperation(
        'generate',
        {},
        'タスクをスケジュール中',
      );
    });

    expect(mockedTakusuServerModule.runScheduleOperation).toHaveBeenCalledTimes(
      1,
    );
    const [, operationId] =
      mockedTakusuServerModule.runScheduleOperation.mock.calls[0];
    expect(mockedTakusuServerModule.runScheduleOperation).toHaveBeenCalledWith(
      'generate',
      operationId,
      '{}',
      'https://example.com',
      'token',
      port,
    );
    expect(result.current.scheduleOperation).toEqual({
      operation: 'generate',
      id: operationId,
      label: 'タスクをスケジュール中',
    });
    expect(mockShowTopToast).toHaveBeenCalledWith('タスクをスケジュール中', {
      type: 'loading',
      duration: Infinity,
    });

    mockedTakusuServerModule.getScheduleOperationStatus.mockResolvedValue({
      id: operationId,
      status: 'succeeded',
      operation: 'generate',
    });

    await act(async () => {
      jest.advanceTimersByTime(500);
    });

    await waitFor(() => expect(mockRefresh).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(mockClient.triggerSync).toHaveBeenCalledTimes(1),
    );
    expect(
      mockedTakusuServerModule.clearScheduleOperationStatus,
    ).toHaveBeenCalled();
    expect(mockShowTopToast).toHaveBeenCalledWith(
      'Google Calendarへ同期しました',
      { type: 'success' },
    );
    expect(mockShowTopToast).toHaveBeenCalledWith(
      'スケジュールを更新しました',
      { type: 'success' },
    );
  });

  it('shows an error when reschedule fails', async () => {
    const { result } = await renderUseScheduleOperation();

    await act(() => {
      result.current.startScheduleOperation(
        'reschedule',
        { mode: 'range' },
        '再スケジュール中',
      );
    });

    const [, operationId] =
      mockedTakusuServerModule.runScheduleOperation.mock.calls[0];
    mockedTakusuServerModule.getScheduleOperationStatus.mockResolvedValue({
      id: operationId,
      status: 'failed',
      operation: 'reschedule',
      message: 'timed out',
    });

    await act(async () => {
      jest.advanceTimersByTime(500);
    });

    await waitFor(() =>
      expect(mockShowTopToast).toHaveBeenCalledWith('timed out', {
        type: 'error',
        duration: 5000,
      }),
    );
    expect(mockRefresh).not.toHaveBeenCalled();
    expect(result.current.lastCompletedAt).toBeNull();
  });

  it('refreshes and updates lastCompletedAt when reschedule succeeds', async () => {
    const { result } = await renderUseScheduleOperation();

    await act(() => {
      result.current.startScheduleOperation(
        'reschedule',
        { mode: 'range' },
        '再スケジュール中',
      );
    });

    const [, operationId] =
      mockedTakusuServerModule.runScheduleOperation.mock.calls[0];
    mockedTakusuServerModule.getScheduleOperationStatus.mockResolvedValue({
      id: operationId,
      status: 'succeeded',
      operation: 'reschedule',
    });

    await act(async () => {
      jest.advanceTimersByTime(500);
    });

    await waitFor(() => expect(mockRefresh).toHaveBeenCalledTimes(1));
    expect(mockClient.triggerSync).not.toHaveBeenCalled();
    expect(
      mockedTakusuServerModule.clearScheduleOperationStatus,
    ).toHaveBeenCalled();
    expect(mockShowTopToast).toHaveBeenCalledWith(
      'スケジュールを更新しました',
      { type: 'success' },
    );
    expect(result.current.lastCompletedAt).not.toBeNull();
  });

  it('does not enqueue when workersUrl or workersToken is missing', async () => {
    const { result } = await renderHook(() =>
      useScheduleOperation({
        client: mockClient,
        workersUrl: undefined,
        workersToken: undefined,
        refresh: mockRefresh,
        showTopToast: mockShowTopToast,
        hideTopToast: mockHideTopToast,
      }),
    );

    await act(() => {
      result.current.startScheduleOperation('generate', {}, 'label');
    });

    expect(
      mockedTakusuServerModule.runScheduleOperation,
    ).not.toHaveBeenCalled();
    expect(mockShowTopToast).toHaveBeenCalledWith(
      'Workers URL またはトークンが設定されていません',
      { type: 'error', duration: 5000 },
    );
  });

  it('handles completion detected on AppState active', async () => {
    const { result } = await renderUseScheduleOperation();

    await act(() => {
      result.current.startScheduleOperation('generate', {}, 'label');
    });

    const [, operationId] =
      mockedTakusuServerModule.runScheduleOperation.mock.calls[0];
    mockedTakusuServerModule.getScheduleOperationStatus.mockResolvedValue({
      id: operationId,
      status: 'succeeded',
      operation: 'generate',
    });

    expect(appStateHandlers).toHaveLength(1);

    await act(async () => {
      appStateHandlers[0]('active');
    });

    await waitFor(() => expect(mockRefresh).toHaveBeenCalledTimes(1));
    expect(mockClient.triggerSync).toHaveBeenCalledTimes(1);
  });

  it('ignores a stale status with a different operation id', async () => {
    const { result } = await renderUseScheduleOperation();

    await act(() => {
      result.current.startScheduleOperation(
        'reschedule',
        { mode: 'range' },
        'label',
      );
    });

    mockedTakusuServerModule.getScheduleOperationStatus.mockResolvedValue({
      id: 'stale-id',
      status: 'succeeded',
      operation: 'reschedule',
    });

    await act(async () => {
      jest.advanceTimersByTime(500);
    });

    expect(mockRefresh).not.toHaveBeenCalled();
    expect(mockShowTopToast).not.toHaveBeenCalledWith(
      'スケジュールを更新しました',
      { type: 'success' },
    );
    expect(result.current.scheduleOperation).not.toBeNull();
  });
});
