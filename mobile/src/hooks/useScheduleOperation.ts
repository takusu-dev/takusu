import { useCallback, useEffect, useRef, useState } from 'react';
import { AppState } from 'react-native';

import { logError } from '@/src/api/errors';
import TakusuServerModule from '@/modules/takusu-server/src/TakusuServerModule';

export interface ScheduleOperation {
  operation: 'generate' | 'reschedule';
  id: string;
  label: string;
}

interface UseScheduleOperationOptions {
  client: { triggerSync: () => Promise<unknown> } | null;
  workersUrl?: string;
  workersToken?: string;
  refresh: () => Promise<void>;
  setStatusLabel: (label: string | null) => void;
  showError: (error: unknown, context: string) => void;
}

export function useScheduleOperation({
  client,
  workersUrl,
  workersToken,
  refresh,
  setStatusLabel,
  showError,
}: UseScheduleOperationOptions) {
  const [scheduleOperation, setScheduleOperation] =
    useState<ScheduleOperation | null>(null);
  const [lastCompletedAt, setLastCompletedAt] = useState<number | null>(null);
  const processedOperationIdRef = useRef<string | null>(null);

  const withStatus = useCallback(
    async <T>(label: string, fn: () => Promise<T>): Promise<T> => {
      setStatusLabel(label);
      try {
        return await fn();
      } finally {
        setStatusLabel(null);
      }
    },
    [setStatusLabel],
  );

  const runGCalSync = useCallback(async () => {
    if (!client) return;
    await withStatus('GCal同期中', () =>
      client.triggerSync().catch((e) => logError('Google Calendar同期', e)),
    );
  }, [client, withStatus]);

  function generateOperationId(operation: string): string {
    try {
      const cryptoLike = (
        globalThis as { crypto?: { randomUUID?: () => string } }
      ).crypto;
      if (cryptoLike?.randomUUID) {
        return `${operation}-${cryptoLike.randomUUID()}`;
      }
    } catch {
      // fall through
    }
    return `${operation}-${Date.now()}-${Math.random().toString(36).slice(2)}`;
  }

  const startScheduleOperation = useCallback(
    (
      operation: 'generate' | 'reschedule',
      params: Record<string, unknown>,
      label: string,
    ) => {
      if (!workersUrl || !workersToken) {
        showError('Workers URL またはトークンが設定されていません', label);
        return;
      }
      const id = generateOperationId(operation);
      try {
        TakusuServerModule.runScheduleOperation(
          operation,
          id,
          JSON.stringify(params),
          workersUrl,
          workersToken,
        );
        setScheduleOperation({ operation, id, label });
      } catch (e) {
        showError(e, label);
      }
    },
    [workersUrl, workersToken, showError],
  );

  const handleCompleted = useCallback(
    async (status: {
      id?: string;
      status: 'running' | 'succeeded' | 'failed' | 'none';
      operation?: string;
      message?: string;
    }) => {
      if (status.status !== 'succeeded' && status.status !== 'failed') {
        return;
      }
      if (!status.id || processedOperationIdRef.current === status.id) {
        return;
      }
      processedOperationIdRef.current = status.id;
      setScheduleOperation(null);
      setStatusLabel(null);
      try {
        TakusuServerModule.clearScheduleOperationStatus();
      } catch {
        // ignore cleanup failure
      }
      if (status.status === 'succeeded') {
        if (status.operation === 'generate') {
          await runGCalSync();
        }
        await refresh();
        setLastCompletedAt(Date.now());
      } else {
        showError(
          status.message || 'スケジュール処理に失敗しました',
          'スケジュール処理',
        );
      }
    },
    [refresh, setStatusLabel, showError, runGCalSync],
  );

  // Poll the status of an active background schedule operation.
  useEffect(() => {
    if (!scheduleOperation) {
      setStatusLabel(null);
      return;
    }
    setStatusLabel(scheduleOperation.label);
    const interval = setInterval(async () => {
      try {
        const status = await TakusuServerModule.getScheduleOperationStatus();
        if (
          status.id === scheduleOperation.id &&
          status.status !== 'running' &&
          status.status !== 'none'
        ) {
          clearInterval(interval);
          await handleCompleted(status);
        }
      } catch {
        // Retry on the next tick.
      }
    }, 500);
    return () => clearInterval(interval);
  }, [scheduleOperation, setStatusLabel, handleCompleted]);

  // When the app returns to the foreground, check whether a background
  // schedule operation finished while away.
  useEffect(() => {
    const subscription = AppState.addEventListener('change', (nextAppState) => {
      if (nextAppState !== 'active') return;
      TakusuServerModule.getScheduleOperationStatus()
        .then(handleCompleted)
        .catch(() => {});
    });
    return () => subscription.remove();
  }, [handleCompleted]);

  return { startScheduleOperation, scheduleOperation, lastCompletedAt };
}
