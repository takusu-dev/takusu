// Types mirroring takusu-client/src/lib.rs and takusu-storage/src/model.rs

export type TaskStatus = 'pending' | 'scheduled' | 'in_progress' | 'completed' | 'skipped';

export interface TaskRow {
  id: string;
  display_id: number;
  title: string;
  description?: string;
  start_at?: string;
  end_at: string;
  avg_minutes: number;
  sigma_minutes: number;
  depends: string; // JSON-encoded Vec<String>
  parallelizable: boolean;
  allows_parallel: boolean;
  abandonability: number;
  status: TaskStatus;
  habit_id?: string;
  ical_uid?: string;
  created_at: string;
  updated_at: string;
}

export interface CreateTask {
  title: string;
  description?: string;
  start_at?: string;
  end_at: string;
  avg_minutes: number;
  sigma_minutes?: number;
  depends?: string[];
  parallelizable?: boolean;
  allows_parallel?: boolean;
  abandonability?: number;
  ical_uid?: string;
  habit_id?: string;
}

export interface UpdateTask {
  title?: string;
  description?: string;
  start_at?: string;
  end_at?: string;
  avg_minutes?: number;
  sigma_minutes?: number;
  depends?: string[];
  parallelizable?: boolean;
  allows_parallel?: boolean;
  abandonability?: number;
  status?: TaskStatus;
}

export interface TaskQuery {
  status?: TaskStatus;
  from?: string;
  until?: string;
  habit_id?: string;
}

export interface HabitRow {
  id: string;
  title: string;
  description?: string;
  recurrence: string;
  start_time: string;
  end_time: string;
  avg_minutes: number;
  sigma_minutes: number;
  parallelizable: boolean;
  allows_parallel: boolean;
  abandonability: number;
  active: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreateHabit {
  title: string;
  description?: string;
  recurrence: string;
  start_time: string;
  end_time: string;
  avg_minutes: number;
  sigma_minutes?: number;
  parallelizable?: boolean;
  allows_parallel?: boolean;
  abandonability?: number;
}

export interface UpdateHabit {
  title?: string;
  description?: string;
  recurrence?: string;
  start_time?: string;
  end_time?: string;
  avg_minutes?: number;
  sigma_minutes?: number;
  parallelizable?: boolean;
  allows_parallel?: boolean;
  abandonability?: number;
  active?: boolean;
}

export interface ScheduleEntry {
  task_id: string;
  start_at: string;
  end_at: string;
}

export interface ScheduleRow {
  id: string;
  created_at: string;
  updated_at: string;
  schedule: string; // JSON-encoded ScheduleEntry[]
}

export interface GenerateSchedule {
  task_ids?: string[];
  until: string;
  sleep?: string;
}

export interface RescheduleRequest {
  range_start: string;
  range_end: string;
  pinned_task_ids?: string[];
}

export interface MoveEntryRequest {
  start_at: string;
  force?: boolean;
}

export interface SettingsRow {
  tz: string;
  sleep_start: string;
  sleep_end: string;
}

export interface UpdateSettings {
  tz?: string;
  sleep_start?: string;
  sleep_end?: string;
}

export interface TokenRow {
  id: string;
  description?: string;
  created_at: string;
  revoked: boolean;
}

export interface TokenCreateResponse {
  token: string;
  id: string;
}

// ── Google Calendar Sync ──

export interface GoogleCalSettings {
  enabled: boolean;
  calendar_id: string;
  client_id: string;
  has_client_secret: boolean;
  has_refresh_token: boolean;
}

export interface UpdateGoogleCalSettings {
  enabled?: boolean;
  calendar_id?: string;
  client_id?: string;
  client_secret?: string;
  refresh_token?: string;
}

export interface OAuthUrlResponse {
  url: string;
}

export interface OAuthCallbackResponse {
  refresh_token_set: boolean;
}

export interface SyncTriggerResponse {
  status: string;
}

export interface GoogleCalEventMapping {
  task_id: string;
  google_event_id: string;
}

// Helper: parse depends JSON string
export function parseDepends(depends: string): string[] {
  try {
    const parsed = JSON.parse(depends);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

// Helper: parse schedule JSON string
export function parseSchedule(schedule: string): ScheduleEntry[] {
  try {
    const parsed = JSON.parse(schedule);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}
