import { TakusuClient } from '@/src/api/client';

describe('TakusuClient', () => {
  const fetchMock = jest.fn();

  beforeEach(() => {
    jest.resetAllMocks();
    (globalThis as any).fetch = fetchMock;
    fetchMock.mockResolvedValue({
      status: 200,
      text: jest.fn().mockResolvedValue('[]'),
    });
  });

  afterEach(() => {
    fetchMock.mockRestore();
  });

  it('listTasks includes no_overdue query parameter', async () => {
    const client = new TakusuClient('http://localhost', 'token');
    await client.listTasks({ no_overdue: true });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const url = fetchMock.mock.calls[0][0] as string;
    expect(url).toBe('http://localhost/api/tasks?no_overdue=true');
  });

  it('listTasks sends no_overdue=false when explicitly false', async () => {
    const client = new TakusuClient('http://localhost', 'token');
    await client.listTasks({ no_overdue: false });
    const url = fetchMock.mock.calls[0][0] as string;
    expect(url).toBe('http://localhost/api/tasks?no_overdue=false');
  });

  it('listTasks omits no_overdue when not provided', async () => {
    const client = new TakusuClient('http://localhost', 'token');
    await client.listTasks({});
    const url = fetchMock.mock.calls[0][0] as string;
    expect(url).toBe('http://localhost/api/tasks');
  });

  it('encodes ids containing reserved URL characters', async () => {
    const client = new TakusuClient('http://localhost', 'token');
    fetchMock.mockResolvedValueOnce({
      status: 200,
      text: jest.fn().mockResolvedValue('{}'),
    });
    await client.getTask('foo#bar');
    const url = fetchMock.mock.calls[0][0] as string;
    expect(url).toBe('http://localhost/api/tasks/foo%23bar');
  });
});
