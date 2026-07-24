import { useCallback, useEffect, useRef, useState } from 'react';
import { AppState } from 'react-native';

import { logError } from '@/src/api/errors';
import type { ToastOptions } from '@/src/components/TopToast';
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
  showTopToast: (message: string, options?: number | ToastOptions) => string;
  hideTopToast: (id: string) => void;
}

export function useScheduleOperation({
  client,
  workersUrl,
  workersToken,
  refresh,
  showTopToast,
  hideTopToast,
}: UseScheduleOperationOptions) {
  const [scheduleOperation, setScheduleOperation] =
    useState<ScheduleOperation | null>(null);
  const [lastCompletedAt, setLastCompletedAt] = useState<number | null>(null);
  const processedOperationIdRef = useRef<string | null>(null);
  const toastIdsRef = useRef(new Map<string, string>());

  const hideToastForOperation = useCallback(
    (operationId: string) => {
      const toastId = toastIdsRef.current.get(operationId);
      if (toastId) {
        hideTopToast(toastId);
        toastIdsRef.current.delete(operationId);
      }
    },
    [hideTopToast],
  );

  const runGCalSync = useCallback(async () => {
    if (!client) return;
    const toastId = showTopToast('Google Calendar同期中', {
      type: 'loading',
      duration: Infinity,
    });
    try {
      await client.triggerSync();
      hideTopToast(toastId);
      showTopToast('Google Calendarへ同期しました', { type: 'success' });
    } catch (e) {
      hideTopToast(toastId);
      showTopToast(e instanceof Error ? e.message : String(e), {
        type: 'error',
        duration: 5000,
      });
      logError('Google Calendar同期', e);
    }
  }, [client, showTopToast, hideTopToast]);

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
      if (scheduleOperation) return;
      if (!workersUrl || !workersToken) {
        showTopToast('Workers URL またはトークンが設定されていません', {
          type: 'error',
          duration: 5000,
        });
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
        const toastId = showTopToast(label, {
          type: 'loading',
          duration: Infinity,
        });
        toastIdsRef.current.set(id, toastId);
        setScheduleOperation({ operation, id, label });
      } catch (e) {
        showTopToast(e instanceof Error ? e.message : String(e), {
          type: 'error',
          duration: 5000,
        });
      }
    },
    [workersUrl, workersToken, showTopToast, scheduleOperation],
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
      hideToastForOperation(status.id);

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
        showTopToast('スケジュールを更新しました', { type: 'success' });
        setLastCompletedAt(Date.now());
      } else {
        showTopToast(status.message || 'スケジュール処理に失敗しました', {
          type: 'error',
          duration: 5000,
        });
      }
    },
    [refresh, showTopToast, runGCalSync, hideToastForOperation],
  );

  // Poll the status of an active background schedule operation.
  useEffect(() => {
    if (!scheduleOperation) {
      return;
    }
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
    return () => {
      clearInterval(interval);
      hideToastForOperation(scheduleOperation.id);
    };
  }, [scheduleOperation, handleCompleted, hideToastForOperation]);

  // When the app returns to the foreground, check whether a background
  // operation finished while away.
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
