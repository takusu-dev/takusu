import type {
  TaskRow,
  CreateTask,
  UpdateTask,
  TaskQuery,
  HabitRow,
  HabitDetail,
  CreateHabit,
  UpdateHabit,
  HabitScheduledSpanRow,
  CreateHabitScheduledSpan,
  HabitStepRow,
  HabitStepInput,
  ScheduleRow,
  GenerateSchedule,
  RescheduleRequest,
  MoveEntryRequest,
  SettingsRow,
  UpdateSettings,
  TokenRow,
  TokenCreateResponse,
  GoogleCalSettings,
  UpdateGoogleCalSettings,
  SyncTriggerResponse,
  DeleteAllGcalResponse,
  GoogleCalEventMapping,
  IcalImportResult,
  DependencyAnalysisResponse,
  SkillRow,
  CreateSkill,
  UpdateSkill,
} from './types';

export class ApiError extends Error {
  constructor(
    public status: number,
    public body: string,
  ) {
    super(`API error ${status}: ${body}`);
    this.name = 'ApiError';
  }
}

export class TakusuClient {
  private baseUrl: string;
  private token: string;

  constructor(baseUrl: string, token: string) {
    this.baseUrl = baseUrl.replace(/\/+$/, '');
    this.token = token;
  }

  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
  ): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const headers: Record<string, string> = {
      Authorization: `Bearer ${this.token}`,
    };
    if (body !== undefined) {
      headers['Content-Type'] = 'application/json';
    }
    const resp = await fetch(url, {
      method,
      headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
    const status = resp.status;
    if (status >= 400) {
      const text = await resp.text().catch(() => '');
      throw new ApiError(status, text);
    }
    const text = await resp.text();
    if (!text) return undefined as T;
    return JSON.parse(text) as T;
  }

  // ── Health ──
  async health(): Promise<string> {
    const resp = await fetch(`${this.baseUrl}/health`);
    return resp.text();
  }

  // ── Task ──
  async listTasks(query?: TaskQuery): Promise<TaskRow[]> {
    // Build the query string manually: Hermes does not provide a working
    // URLSearchParams (its methods throw or are missing at runtime).
    const params: string[] = [];
    if (query?.status)
      params.push(`status=${encodeURIComponent(query.status)}`);
    if (query?.from) params.push(`from=${encodeURIComponent(query.from)}`);
    if (query?.until) params.push(`until=${encodeURIComponent(query.until)}`);
    if (query?.habit_id)
      params.push(`habit_id=${encodeURIComponent(query.habit_id)}`);
    const qs = params.join('&');
    return this.request('GET', `/api/tasks${qs ? `?${qs}` : ''}`);
  }

  async getTask(id: string): Promise<TaskRow> {
    return this.request('GET', `/api/tasks/${id}`);
  }

  async createTask(body: CreateTask): Promise<TaskRow> {
    return this.request('POST', '/api/tasks', body);
  }

  async updateTask(id: string, body: UpdateTask): Promise<TaskRow> {
    return this.request('PATCH', `/api/tasks/${id}`, body);
  }

  async replaceTask(id: string, body: CreateTask): Promise<TaskRow> {
    return this.request('PUT', `/api/tasks/${id}`, body);
  }

  async deleteTask(id: string): Promise<void> {
    return this.request('DELETE', `/api/tasks/${id}`);
  }

  // ── Composite dependency analysis (#355) ──
  async analyzeTaskDependencies(): Promise<DependencyAnalysisResponse> {
    return this.request('GET', '/api/tasks/dependency-analysis');
  }

  async importIcal(icalText: string): Promise<IcalImportResult> {
    const url = `${this.baseUrl}/api/tasks/import/ical`;
    const resp = await fetch(url, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${this.token}`,
        'Content-Type': 'text/plain',
      },
      body: icalText,
    });
    const status = resp.status;
    if (status >= 400) {
      const text = await resp.text().catch(() => '');
      throw new ApiError(status, text);
    }
    const text = await resp.text();
    if (!text) return { imported: 0, task_ids: [] };
    return JSON.parse(text) as IcalImportResult;
  }

  // ── Habit ──
  async listHabits(): Promise<HabitRow[]> {
    return this.request('GET', '/api/habits');
  }

  async getHabit(id: string): Promise<HabitDetail> {
    return this.request('GET', `/api/habits/${id}`);
  }

  async createHabit(body: CreateHabit): Promise<HabitRow> {
    return this.request('POST', '/api/habits', body);
  }

  async updateHabit(id: string, body: UpdateHabit): Promise<HabitRow> {
    return this.request('PATCH', `/api/habits/${id}`, body);
  }

  async replaceHabit(id: string, body: CreateHabit): Promise<HabitRow> {
    return this.request('PUT', `/api/habits/${id}`, body);
  }

  async deleteHabit(id: string): Promise<void> {
    return this.request('DELETE', `/api/habits/${id}`);
  }

  // ── Habit scheduled spans (#303 / #503) ──
  async listHabitScheduledSpans(id: string): Promise<HabitScheduledSpanRow[]> {
    return this.request('GET', `/api/habits/${id}/scheduled-spans`);
  }

  async listAllHabitScheduledSpans(): Promise<HabitScheduledSpanRow[]> {
    return this.request('GET', '/api/habits/scheduled-spans');
  }

  async createHabitScheduledSpan(
    id: string,
    body: CreateHabitScheduledSpan,
  ): Promise<HabitScheduledSpanRow> {
    return this.request('POST', `/api/habits/${id}/scheduled-spans`, body);
  }

  async deleteHabitScheduledSpan(id: string, spanId: string): Promise<void> {
    return this.request(
      'DELETE',
      `/api/habits/${id}/scheduled-spans/${spanId}`,
    );
  }

  // ── Habit steps (#95) ──
  async listHabitSteps(id: string): Promise<HabitStepRow[]> {
    return this.request('GET', `/api/habits/${id}/steps`);
  }

  async listAllHabitSteps(): Promise<HabitStepRow[]> {
    return this.request('GET', '/api/habits/steps');
  }

  async replaceHabitSteps(
    id: string,
    steps: HabitStepInput[],
  ): Promise<HabitStepRow[]> {
    return this.request('PUT', `/api/habits/${id}/steps`, steps);
  }

  async analyzeHabitStepDependencies(
    id: string,
  ): Promise<DependencyAnalysisResponse> {
    return this.request('GET', `/api/habits/${id}/steps/dependency-analysis`);
  }

  // ── Schedule ──
  async getSchedule(): Promise<ScheduleRow> {
    return this.request('GET', '/api/schedule');
  }

  async generateSchedule(body: GenerateSchedule): Promise<ScheduleRow> {
    return this.request('POST', '/api/schedule/generate', body);
  }

  async reschedule(body: RescheduleRequest): Promise<ScheduleRow> {
    return this.request('POST', '/api/schedule/reschedule', body);
  }

  async moveEntry(
    taskId: string,
    body: MoveEntryRequest,
  ): Promise<ScheduleRow> {
    return this.request('PATCH', `/api/schedule/entries/${taskId}`, body);
  }

  async clearSchedule(): Promise<void> {
    return this.request('DELETE', '/api/schedule');
  }

  // ── Settings ──
  async getSettings(): Promise<SettingsRow> {
    return this.request('GET', '/api/settings');
  }

  async updateSettings(body: UpdateSettings): Promise<SettingsRow> {
    return this.request('PUT', '/api/settings', body);
  }

  // ── Token ──
  async listTokens(): Promise<TokenRow[]> {
    return this.request('GET', '/api/tokens');
  }

  async createToken(description?: string): Promise<TokenCreateResponse> {
    return this.request('POST', '/api/tokens', { description });
  }

  async revokeToken(id: string): Promise<void> {
    return this.request('DELETE', `/api/tokens/${id}`);
  }

  // ── Sync / Google Calendar ──
  async getGcalSettings(): Promise<GoogleCalSettings> {
    return this.request('GET', '/api/sync/settings');
  }

  async updateGcalSettings(
    body: UpdateGoogleCalSettings,
  ): Promise<GoogleCalSettings> {
    return this.request('PUT', '/api/sync/settings', body);
  }

  async triggerSync(): Promise<SyncTriggerResponse> {
    return this.request('POST', '/api/sync/trigger');
  }

  async deleteAllGcalEvents(): Promise<DeleteAllGcalResponse> {
    return this.request('POST', '/api/sync/delete-all');
  }

  async listGcalMappings(): Promise<GoogleCalEventMapping[]> {
    return this.request('GET', '/api/sync/mappings');
  }

  // ── Skills (#WI-6) ──
  async listSkills(): Promise<SkillRow[]> {
    return this.request('GET', '/api/skills');
  }

  async getSkill(slug: string): Promise<SkillRow> {
    return this.request('GET', `/api/skills/${slug}`);
  }

  async createSkill(body: CreateSkill): Promise<SkillRow> {
    return this.request('POST', '/api/skills', body);
  }

  async updateSkill(slug: string, body: UpdateSkill): Promise<SkillRow> {
    return this.request('PATCH', `/api/skills/${slug}`, body);
  }

  async deleteSkill(slug: string): Promise<void> {
    return this.request('DELETE', `/api/skills/${slug}`);
  }

  // ── Health ──
  async workerHealthCheck(): Promise<{ status: string }> {
    return this.request('GET', '/api/workers/health');
  }
}
