import TakusuServerModule from '@/modules/takusu-server/src/TakusuServerModule';
import { ensureLocalServer, DEFAULT_LOCAL_PORT } from '@/src/api/server';

jest.mock('@/modules/takusu-server/src/TakusuServerModule', () => ({
  status: jest.fn(),
  start: jest.fn(),
}));

const mockedModule = TakusuServerModule as unknown as {
  status: jest.Mock;
  start: jest.Mock;
};

describe('ensureLocalServer', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it('reuses a running server and returns a client for the reported port', () => {
    mockedModule.status.mockReturnValue({ running: true, port: 4242 });
    const client = ensureLocalServer({
      workersUrl: 'https://example.com',
      rootToken: 'token',
    });
    expect(mockedModule.start).not.toHaveBeenCalled();
    expect((client as any).baseUrl).toBe('http://127.0.0.1:4242');
  });

  it('starts the server on the default port when it is not running', () => {
    mockedModule.status
      .mockReturnValueOnce({ running: false, port: 0 })
      .mockReturnValueOnce({ running: true, port: DEFAULT_LOCAL_PORT });
    const client = ensureLocalServer({
      workersUrl: 'https://example.com',
      rootToken: 'token',
    });
    expect(mockedModule.start).toHaveBeenCalledWith(
      expect.objectContaining({
        port: DEFAULT_LOCAL_PORT,
        workersUrl: 'https://example.com',
        rootToken: 'token',
      }),
    );
    expect((client as any).baseUrl).toBe(
      `http://127.0.0.1:${DEFAULT_LOCAL_PORT}`,
    );
  });

  it('uses a custom port when provided', () => {
    mockedModule.status
      .mockReturnValueOnce({ running: false, port: 0 })
      .mockReturnValueOnce({ running: true, port: 5000 });
    const client = ensureLocalServer({
      port: 5000,
      workersUrl: 'https://example.com',
      rootToken: 'token',
    });
    expect(mockedModule.start).toHaveBeenCalledWith(
      expect.objectContaining({ port: 5000 }),
    );
    expect((client as any).baseUrl).toBe('http://127.0.0.1:5000');
  });

  it('reuses the server when start reports it is already running', () => {
    mockedModule.status
      .mockReturnValueOnce({ running: false, port: 0 })
      .mockReturnValueOnce({ running: true, port: DEFAULT_LOCAL_PORT });
    mockedModule.start.mockImplementation(() => {
      throw new Error('already running');
    });
    const client = ensureLocalServer({
      workersUrl: 'https://example.com',
      rootToken: 'token',
    });
    expect(mockedModule.start).toHaveBeenCalled();
    expect((client as any).baseUrl).toBe(
      `http://127.0.0.1:${DEFAULT_LOCAL_PORT}`,
    );
  });

  it('rethrows non-already-running start errors', () => {
    mockedModule.status.mockReturnValue({ running: false, port: 0 });
    mockedModule.start.mockImplementation(() => {
      throw new Error('port in use');
    });
    expect(() =>
      ensureLocalServer({
        workersUrl: 'https://example.com',
        rootToken: 'token',
      }),
    ).toThrow('port in use');
  });

  it('throws when the server does not report running after start', () => {
    mockedModule.status
      .mockReturnValueOnce({ running: false, port: 0 })
      .mockReturnValueOnce({ running: false, port: 0 });
    mockedModule.start.mockReturnValue(true);
    expect(() =>
      ensureLocalServer({
        workersUrl: 'https://example.com',
        rootToken: 'token',
      }),
    ).toThrow('Local server did not start');
  });

  it('passes agentConfigJson when provided', () => {
    mockedModule.status
      .mockReturnValueOnce({ running: false, port: 0 })
      .mockReturnValueOnce({ running: true, port: DEFAULT_LOCAL_PORT });
    ensureLocalServer({
      workersUrl: 'https://example.com',
      rootToken: 'token',
      agentConfigJson: '{"llm":{}}',
    });
    expect(mockedModule.start).toHaveBeenCalledWith(
      expect.objectContaining({ agentConfigJson: '{"llm":{}}' }),
    );
  });
});
