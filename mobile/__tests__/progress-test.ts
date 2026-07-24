import {
  recordProgressWithTotal,
  makeProgressOperationId,
} from '@/src/utils/progress';
import type { TakusuClient } from '@/src/api/client';
import type { TaskRow } from '@/src/api/types';

describe('recordProgressWithTotal', () => {
  const task: TaskRow = {
    id: 'task-1',
    display_id: 1,
    title: 'Task',
    end_at: '2026-06-05T18:00:00+09:00',
    avg_minutes: 30,
    sigma_minutes: 0,
    depends: '[]',
    parallelizable: false,
    allows_parallel: false,
    abandonability: 0.5,
    status: 'in_progress',
    user_edited: false,
    fixed: false,
    quantity_total: 10,
    quantity_done: 0,
    created_at: '2026-06-01T00:00:00Z',
    updated_at: '2026-06-01T00:00:00Z',
  };

  function makeClient(
    overrides?: Partial<Record<keyof TakusuClient, unknown>>,
  ): TakusuClient {
    return {
      updateTask: jest.fn().mockResolvedValue(undefined),
      recordProgress: jest.fn().mockResolvedValue({} as never),
      ...overrides,
    } as unknown as TakusuClient;
  }

  it('generates and passes an operationId to recordProgress', async () => {
    const client = makeClient();
    const returned = await recordProgressWithTotal(client, task, {
      quantityDone: 5,
      note: 'done',
    });
    expect(client.updateTask).not.toHaveBeenCalled();
    expect(client.recordProgress).toHaveBeenCalledWith(
      'task-1',
      { quantity_done: 5, note: 'done' },
      expect.stringMatching(/./),
    );
    expect(returned).toBe(
      (client.recordProgress as jest.Mock).mock.calls[0][2],
    );
  });

  it('uses the provided operationId when given', async () => {
    const client = makeClient();
    const operationId = 'op-123';
    const returned = await recordProgressWithTotal(
      client,
      task,
      { quantityDone: 3 },
      { operationId },
    );
    expect(client.recordProgress).toHaveBeenCalledWith(
      'task-1',
      { quantity_done: 3, note: undefined },
      operationId,
    );
    expect(returned).toBe(operationId);
  });

  it('skips quantity_total update when it matches the current total', async () => {
    const client = makeClient();
    await recordProgressWithTotal(client, task, {
      quantityDone: 5,
      quantityTotal: 10,
    });
    expect(client.updateTask).not.toHaveBeenCalled();
  });

  it('reverts quantity_total when recordProgress fails', async () => {
    const client = makeClient({
      recordProgress: jest.fn().mockRejectedValue(new Error('network')),
    });
    await expect(
      recordProgressWithTotal(client, task, {
        quantityDone: 5,
        quantityTotal: 20,
      }),
    ).rejects.toThrow('network');
    expect(client.updateTask).toHaveBeenCalledTimes(2);
    expect(client.updateTask).toHaveBeenLastCalledWith('task-1', {
      quantity_total: 10,
    });
  });
});

describe('makeProgressOperationId', () => {
  it('returns a non-empty string', () => {
    const id = makeProgressOperationId();
    expect(typeof id).toBe('string');
    expect(id.length).toBeGreaterThan(0);
  });
});
