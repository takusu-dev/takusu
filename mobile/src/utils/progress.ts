import type { TakusuClient } from '@/src/api/client';
import type { TaskRow } from '@/src/api/types';

export interface ProgressPayload {
  quantityDone: number;
  note?: string;
  quantityTotal?: number;
}

export interface RecordProgressOptions {
  operationId?: string;
}

const ALPHANUM =
  'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';

function randomAlphanumeric(length: number): string {
  let result = '';
  for (let i = 0; i < length; i += 1) {
    result += ALPHANUM[Math.floor(Math.random() * ALPHANUM.length)];
  }
  return result;
}

export function makeProgressOperationId(): string {
  try {
    const cryptoLike = (
      globalThis as { crypto?: { randomUUID?: () => string } }
    ).crypto;
    if (cryptoLike?.randomUUID) {
      return cryptoLike.randomUUID();
    }
  } catch {
    // fall through
  }
  return `${Date.now()}-${randomAlphanumeric(16)}`;
}

// Update quantity_total if it changed, then record progress.
// Centralizes the quantity update + recordProgress flow used by HomeView and
// TaskDetailView so the logic does not diverge between pause/record paths.
export async function recordProgressWithTotal(
  client: TakusuClient,
  task: TaskRow,
  payload: ProgressPayload,
  options?: RecordProgressOptions,
): Promise<string> {
  const operationId = options?.operationId ?? makeProgressOperationId();
  const originalTotal = task.quantity_total;
  let totalUpdated = false;
  if (
    payload.quantityTotal !== undefined &&
    payload.quantityTotal !== originalTotal
  ) {
    await client.updateTask(task.id, {
      quantity_total: payload.quantityTotal,
    });
    totalUpdated = true;
  }
  try {
    await client.recordProgress(
      task.id,
      {
        quantity_done: payload.quantityDone,
        note: payload.note,
      },
      operationId,
    );
  } catch (e) {
    if (totalUpdated && originalTotal !== undefined) {
      // Best-effort rollback so a failed progress event does not leave a
      // partially updated quantity_total behind.
      await client
        .updateTask(task.id, {
          quantity_total: originalTotal,
        })
        .catch(() => {
          // Ignore rollback failure; the original error is what matters.
        });
    }
    throw e;
  }
  return operationId;
}
