import { NativeModule, requireNativeModule } from 'expo';

export interface WidgetConfig {
  workersUrl: string;
  token: string;
}

export interface WidgetSnapshotData {
  doingTitles: string[];
  upcoming: {
    id: string;
    title: string;
    startAt: string | null;
    endAt: string;
  }[];
  unscheduledCount: number;
}

interface TakusuWidgetModuleType extends NativeModule {
  // Persist the Workers URL + token into SharedPreferences so the
  // AppWidgetProvider can read them without going through the JS runtime.
  // The widget calls the Workers API directly via HTTP, so it needs these
  // credentials available even when the app process is not running.
  saveConfig(config: WidgetConfig): boolean;
  // Persist a snapshot (fetched by the JS side) into SharedPreferences and
  // immediately refresh the widget. This ensures the widget shows fresh
  // data right after the app refreshes, without waiting for WorkManager.
  saveSnapshot(snapshot: WidgetSnapshotData): boolean;
  // Force a one-shot widget re-render from the cached snapshot (without
  // writing new data). Used when only a visual refresh is needed.
  requestUpdate(): boolean;
}

const TakusuWidgetModule =
  requireNativeModule<TakusuWidgetModuleType>('TakusuWidget');

export default TakusuWidgetModule;
