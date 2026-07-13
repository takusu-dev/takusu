import type {
  ApprovalRequest,
  ApprovalResult,
  ChangeReceipt,
} from './agentTypes';

export interface AgentTurnResult {
  text: string;
  changes: ChangeReceipt[];
  schedule_dirty: boolean;
  approval_request: ApprovalRequest | null;
}

export interface AgentCapabilities {
  audio_input: boolean;
  tts: boolean;
  approvals: boolean;
}

export class AgentApiError extends Error {
  constructor(
    public status: number,
    public body: string,
  ) {
    super(`Agent API error ${status}: ${body}`);
    this.name = 'AgentApiError';
  }
}

export class AgentClient {
  private readonly baseUrl: string;
  private readonly token: string;

  constructor(baseUrl: string, token: string) {
    this.baseUrl = baseUrl.replace(/\/+$/, '');
    this.token = token;
  }

  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
  ): Promise<T> {
    const response = await fetch(`${this.baseUrl}${path}`, {
      method,
      headers: {
        Authorization: `Bearer ${this.token}`,
        ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await response.text().catch(() => '');
    if (!response.ok) throw new AgentApiError(response.status, text);
    return text ? (JSON.parse(text) as T) : (undefined as T);
  }

  async health(): Promise<void> {
    await this.request<{ ok: boolean }>('GET', '/api/agent/v1/health');
  }

  async capabilities(): Promise<AgentCapabilities> {
    return this.request<AgentCapabilities>('GET', '/api/agent/v1/capabilities');
  }

  async createSession(): Promise<string> {
    const response = await this.request<{ session_id: string }>(
      'POST',
      '/api/agent/v1/sessions',
    );
    return response.session_id;
  }

  async runTurn(
    sessionId: string,
    text: string,
    idempotencyKey: string,
  ): Promise<AgentTurnResult> {
    return this.request<AgentTurnResult>(
      'POST',
      `/api/agent/v1/sessions/${encodeURIComponent(sessionId)}/turns`,
      { version: 1, text, idempotency_key: idempotencyKey },
    );
  }

  async getApproval(sessionId: string): Promise<ApprovalRequest | null> {
    const response = await this.request<ApprovalRequest | { version: number }>(
      'GET',
      `/api/agent/v1/sessions/${encodeURIComponent(sessionId)}/approval`,
    );
    if (response && 'id' in response && typeof response.id === 'string') {
      return response as ApprovalRequest;
    }
    return null;
  }

  async resolveApproval(
    sessionId: string,
    approvalId: string,
    approve: boolean,
    idempotencyKey: string,
  ): Promise<ApprovalResult> {
    return this.request<ApprovalResult>(
      'POST',
      `/api/agent/v1/sessions/${encodeURIComponent(sessionId)}/approvals/${encodeURIComponent(approvalId)}`,
      { version: 1, approve, idempotency_key: idempotencyKey },
    );
  }

  async deleteSession(sessionId: string): Promise<void> {
    await this.request<void>(
      'DELETE',
      `/api/agent/v1/sessions/${encodeURIComponent(sessionId)}`,
    );
  }
}
