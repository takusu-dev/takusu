// Home (Task) view — the main screen
// Top bar: hamburger menu, search button, sync button
// Middle: task cards in chronological order (pending on top, date separators)
// Bottom: add button (center), start&done button (right)
// Pull-down-to-reveal past days

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ActivityIndicator,
  Alert,
  BackHandler,
  FlatList,
  Pressable,
  StyleSheet,
  Text,
  View,
  RefreshControl,
  useWindowDimensions,
  type ViewStyle,
} from 'react-native';
import { useRouter } from 'expo-router';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useServer } from '@/src/api/ServerProvider';
import { TakusuClient } from '@/src/api/client';
import { undoRedo } from '@/src/api/undoRedo';
import { showError, logError } from '@/src/api/errors';
import type { TaskRow, TaskStatus, ScheduleEntry } from '@/src/api/types';
import { parseDepends, parseSchedule } from '@/src/api/types';
import { TaskCard, ParallelGroupCard } from '@/src/components/TaskCard';
import { NavigationButtons } from '@/src/components/NavigationButtons';
import { ViewChanger, type ViewType } from '@/src/components/ViewChanger';
import { ContextMenu } from '@/src/components/ContextMenu';
import { AddButton } from '@/src/components/AddButton';
import { TaskAddSheet } from '@/src/components/TaskAddSheet';
import { Ionicons } from '@expo/vector-icons';
import Reanimated, {
  useSharedValue,
  useAnimatedStyle,
  withTiming,
} from 'react-native-reanimated';
import { useColors, COLORS, BRAND_COLOR } from '@/src/theme';
import { haptic } from '@/src/components/haptics';
import TakusuWidgetModule from '../../modules/takusu-widget/src/TakusuWidgetModule';
import {
  rescheduleFromRaw,
  postInProgressNotification,
  dismissInProgressNotification,
  dismissTaskNotifications,
} from '@/src/notifications';
import type { HabitRow } from '@/src/api/types';

interface TaskItem {
  type: 'task';
  task: TaskRow;
  scheduleStart?: string;
  scheduleEnd?: string;
  isDone: boolean;
  dateKey: string;
}

interface ParallelGroupItem {
  type: 'parallelGroup';
  host: TaskRow;
  guests: TaskRow[];
  hostScheduleStart?: string;
  hostScheduleEnd?: string;
  guestScheduleStarts: (string | undefined)[];
  guestScheduleEnds: (string | undefined)[];
  dateKey: string;
}

interface DateSeparator {
  type: 'separator';
  label: string;
}

type ListItem = TaskItem | ParallelGroupItem | DateSeparator;

function dateKey(iso: string, tz?: string): string {
  // Convert UTC ISO string to the configured timezone's local date
  // (YYYY-MM-DD). The server's sync_habit_tasks uses the same configured
  // tz for its date keys, so we must match it here to keep date
  // separators consistent with the server's grouping.
  // Falls back to the device timezone if the server tz is unavailable.
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso.slice(0, 10);
  try {
    const fmt = new Intl.DateTimeFormat('en-CA', {
      timeZone: tz || undefined,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
    });
    return fmt.format(d);
  } catch {
    // Invalid tz string — fall back to device-local date
    const y = d.getFullYear();
    const m = (d.getMonth() + 1).toString().padStart(2, '0');
    const day = d.getDate().toString().padStart(2, '0');
    return `${y}-${m}-${day}`;
  }
}

function dateLabel(key: string, tz?: string): string {
  // Compare the date key (already in server tz) against "today" in the
  // same server tz, so the 今日/明日/昨日 labels are consistent with the
  // date separators built by dateKey.
  const d = new Date(key + 'T00:00:00');
  const todayKey = todayDateKey(tz);
  const today = new Date(todayKey + 'T00:00:00');
  const diff = Math.round(
    (d.getTime() - today.getTime()) / (1000 * 60 * 60 * 24),
  );
  if (diff === 0) return '今日';
  if (diff === 1) return '明日';
  if (diff === -1) return '昨日';
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

/// Returns today's date as YYYY-MM-DD in the given timezone (or the
/// device timezone if tz is undefined/invalid). Mirrors dateKey so the
/// "today" used by dateLabel matches the server's date grouping.
function todayDateKey(tz?: string): string {
  return dateKey(new Date().toISOString(), tz);
}

// A separator that marks a real day boundary (今日 / 明日 / M/D). Excludes
// the non-day separators: 'pending', '過去', and the "load more past" row.
function isDaySeparator(item: DateSeparator): boolean {
  return (
    item.label !== 'pending' &&
    item.label !== '過去' &&
    !item.label.startsWith('過去をさらに読み込む')
  );
}

// viewabilityConfig for tracking the topmost visible item index. Module-level
// so the object identity stays stable across renders (FlatList requirement).
const VIEWABILITY_CONFIG = {
  minimumViewTime: 0,
  viewAreaCoveragePercentThreshold: 0,
} as const;
export function HomeView() {
  const { client, notifications } = useServer();
  const router = useRouter();
  const colors = useColors();
  const insets = useSafeAreaInsets();
  const { height: screenHeight } = useWindowDimensions();

  // ── Task-add bottom-sheet preview state ──
  // sheetY drives the sheet's translateY (screenHeight = hidden, 0 = open).
  // sheetMounted controls whether the sheet is rendered at all.
  // sheetOpen controls whether the sheet content is interactive.
  // unmountTimer holds the pending setTimeout id that unmounts the sheet
  // after the close animation; it is cleared whenever a new drag starts so
  // a quick second drag can't have the sheet yanked out from under it.
  const sheetY = useSharedValue(screenHeight);
  const [sheetMounted, setSheetMounted] = useState(false);
  const [sheetOpen, setSheetOpen] = useState(false);
  const unmountTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  function scheduleUnmount(delay = 220) {
    if (unmountTimer.current) clearTimeout(unmountTimer.current);
    unmountTimer.current = setTimeout(() => setSheetMounted(false), delay);
  }

  function cancelUnmount() {
    if (unmountTimer.current) {
      clearTimeout(unmountTimer.current);
      unmountTimer.current = null;
    }
  }

  const [tasks, setTasks] = useState<TaskRow[]>([]);
  const [schedule, setSchedule] = useState<ScheduleEntry[]>([]);
  const [habits, setHabits] = useState<HabitRow[]>([]);
  // habit_id (UUID) → display_id map for habit-based task coloring (#309)
  // and h1#5 ID labels (#305).
  const habitDisplayIdMap = useMemo(
    () => new Map(habits.map((h) => [h.id, h.display_id])),
    [habits],
  );
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [refreshing, setRefreshing] = useState(false);
  // Server-configured timezone (from GET /api/settings). Used by dateKey
  // so date separators match the server's habit sync date grouping.
  const [serverTz, setServerTz] = useState<string | undefined>(undefined);
  // In-progress status label shown in the top-bar center while a
  // scheduling / Google Calendar sync operation is running.
  const [statusLabel, setStatusLabel] = useState<string | null>(null);
  const [view, setView] = useState<ViewType>('task');
  const viewChanger = useMemo(
    () => <ViewChanger current={view} onChange={setView} />,
    [view],
  );
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [showPast, setShowPast] = useState(false);
  // #206: past tasks load 1 week at a time
  const [pastWeeks, setPastWeeks] = useState(1);
  const listRef = useRef<FlatList<ListItem>>(null);
  const scrollOffsetRef = useRef(0);
  // Viewport height of the FlatList (for page-sized scrolls). Captured via
  // onLayout so it stays correct across rotation / keyboard changes.
  const listLayoutHeightRef = useRef(0);
  // Index of the topmost visible item, kept in sync via
  // onViewableItemsChanged. Used by scrollByDay to find the next/previous
  // day separator relative to the current scroll position.
  const visibleTopIndexRef = useRef(0);

  // Navigation buttons visibility — shown when scrolling, hidden when idle
  // (#308). Uses a shared value for smooth opacity animation.
  const navOpacity = useSharedValue(0);
  const [navButtonsVisible, setNavButtonsVisible] = useState(false);
  const navHideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const navDisableTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const showNavButtons = useCallback(() => {
    if (navHideTimer.current) {
      clearTimeout(navHideTimer.current);
      navHideTimer.current = null;
    }
    if (navDisableTimer.current) {
      clearTimeout(navDisableTimer.current);
      navDisableTimer.current = null;
    }
    navOpacity.value = withTiming(1, { duration: 200 });
    setNavButtonsVisible(true);
  }, [navOpacity]);

  const scheduleHideNavButtons = useCallback(() => {
    if (navHideTimer.current) clearTimeout(navHideTimer.current);
    if (navDisableTimer.current) {
      clearTimeout(navDisableTimer.current);
      navDisableTimer.current = null;
    }
    navHideTimer.current = setTimeout(() => {
      navOpacity.value = withTiming(0, { duration: 300 });
      // Disable taps after the fade-out animation completes
      navDisableTimer.current = setTimeout(
        () => setNavButtonsVisible(false),
        350,
      );
    }, 1500);
  }, [navOpacity]);

  const navButtonsStyle = useAnimatedStyle(() => ({
    opacity: navOpacity.value,
  }));

  // Stable callback for FlatList's onViewableItemsChanged. React Native
  // warns ("Changing onViewableItemsChanged on the fly is not supported")
  // when the callback identity changes after mount, so it must be wrapped
  // in useCallback with an empty dependency array. The callback only writes
  // to a ref, so capturing it once is safe.
  const handleViewableItemsChanged = useCallback(
    ({ viewableItems }: { viewableItems: Array<{ index: number | null }> }) => {
      if (viewableItems.length > 0) {
        visibleTopIndexRef.current = viewableItems[0].index ?? 0;
      }
    },
    [],
  );

  // Animated chevron rotation for past-day toggle
  const chevronRotate = useSharedValue(0);
  const chevronStyle = useAnimatedStyle(() => ({
    transform: [{ rotate: `${chevronRotate.value}deg` }],
  }));
  function togglePast() {
    haptic.select();
    setShowPast((v) => {
      const next = !v;
      chevronRotate.value = withTiming(next ? 180 : 0, { duration: 250 });
      if (!next) setPastWeeks(1); // reset pagination when collapsing
      return next;
    });
  }

  const refresh = useCallback(async () => {
    if (!client) return;
    setRefreshing(true);
    try {
      const [taskList, sched, habitList, settings] = await Promise.all([
        client.listTasks(),
        client.getSchedule().catch((e) => {
          logError('スケジュール取得', e);
          return null;
        }),
        client.listHabits().catch((e) => {
          logError('Habit取得', e);
          return [] as HabitRow[];
        }),
        client.getSettings().catch(() => null),
      ]);
      setTasks(taskList);
      setSchedule(sched ? parseSchedule(sched.schedule) : []);
      setHabits(habitList);
      setServerTz(settings?.tz);
      // Push a fresh snapshot to the home screen widget so it shows
      // current data immediately (without waiting for WorkManager).
      try {
        const schedEntries = sched ? parseSchedule(sched.schedule) : [];
        const schedMap = new Map(schedEntries.map((e) => [e.task_id, e]));
        const now = Date.now();
        const doingTitles: string[] = [];
        let unscheduledCount = 0;
        const upcoming: {
          title: string;
          startAt: string | null;
          endAt: string;
        }[] = [];
        for (const t of taskList) {
          if (t.status === 'in_progress') {
            doingTitles.push(t.title);
          } else if (t.status === 'pending') {
            unscheduledCount++;
          } else if (t.status === 'scheduled') {
            const entry = schedMap.get(t.id);
            const startAt = entry?.start_at ?? t.start_at ?? null;
            const endAt = entry?.end_at ?? t.end_at;
            const endTime = new Date(endAt).getTime();
            if (endTime >= now) {
              upcoming.push({ title: t.title, startAt, endAt });
            }
          }
        }
        upcoming.sort((a, b) => {
          const ta = new Date(a.startAt ?? a.endAt).getTime();
          const tb = new Date(b.startAt ?? b.endAt).getTime();
          return ta - tb;
        });
        TakusuWidgetModule.saveSnapshot({
          doingTitles,
          upcoming: upcoming.slice(0, 5),
          unscheduledCount,
        });
      } catch {
        // widget module not available (non-Android) — ignore
      }
    } catch (e) {
      showError(e, 'タスク一覧の取得に失敗');
    } finally {
      setRefreshing(false);
    }
  }, [client]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // Reschedule notifications when tasks, schedule, or notification
  // settings change. This is separate from refresh() to avoid triggering a
  // full server refetch when only notification settings are toggled.
  useEffect(() => {
    if (tasks.length === 0) return;
    rescheduleFromRaw(
      tasks,
      schedule.length > 0 ? JSON.stringify(schedule) : null,
      notifications,
    ).catch((e) => logError('通知の再スケジュール', e));
  }, [tasks, schedule, notifications]);

  // Close the task-add sheet on Android hardware back button.  Without this
  // the back button would navigate away from HomeView (or exit the app)
  // instead of dismissing the sheet overlay.
  // closeAddSheetRef always points at the latest closeAddSheet (which reads
  // the current screenHeight), so a rotation while the sheet is open does
  // not leave the handler animating to a stale height.
  const closeAddSheetRef = useRef<() => void>(() => {});
  closeAddSheetRef.current = closeAddSheet;
  useEffect(() => {
    if (!sheetOpen) return;
    const subscription = BackHandler.addEventListener(
      'hardwareBackPress',
      () => {
        closeAddSheetRef.current();
        return true; // prevent default navigation
      },
    );
    return () => subscription.remove();
  }, [sheetOpen]);

  const scheduleMap = useMemo(() => {
    const m = new Map<string, ScheduleEntry>();
    for (const e of schedule) m.set(e.task_id, e);
    return m;
  }, [schedule]);

  // Build parallel groups: host (allows_parallel=true) → overlapping guests
  // (parallelizable=true). Each guest is assigned to at most one host (the
  // first one found) to avoid duplicate rendering across groups.
  // Only hosts whose schedule end is in the future (upcoming) form groups —
  // a past host would render in the past section (no grouping), so its
  // guests must not be claimed or they'd vanish from the upcoming list.
  const { parallelGroups, groupedGuestIds } = useMemo(() => {
    const groups = new Map<string, TaskRow[]>();
    const guestIds = new Set<string>();
    const now = Date.now();
    const hosts = tasks.filter(
      (t) =>
        t.allows_parallel &&
        t.status !== 'pending' &&
        t.status !== 'completed' &&
        t.status !== 'skipped' &&
        new Date(scheduleMap.get(t.id)?.end_at ?? t.end_at).getTime() >= now,
    );
    const guests = tasks.filter(
      (t) =>
        t.parallelizable &&
        t.status !== 'pending' &&
        t.status !== 'completed' &&
        t.status !== 'skipped',
    );
    for (const host of hosts) {
      // Skip hosts that have already been claimed as a guest by another
      // host — otherwise their own guests would be orphaned (claimed but
      // never rendered, since this host is skipped in the upcoming loop).
      if (guestIds.has(host.id)) continue;
      const hostEntry = scheduleMap.get(host.id);
      if (!hostEntry) continue;
      const hostStart = new Date(hostEntry.start_at).getTime();
      const hostEnd = new Date(hostEntry.end_at).getTime();
      const overlapping: TaskRow[] = [];
      for (const guest of guests) {
        if (guest.id === host.id) continue;
        // Skip guests already claimed by another host.
        if (guestIds.has(guest.id)) continue;
        const guestEntry = scheduleMap.get(guest.id);
        if (!guestEntry) continue;
        const gStart = new Date(guestEntry.start_at).getTime();
        const gEnd = new Date(guestEntry.end_at).getTime();
        if (gStart < hostEnd && gEnd > hostStart) {
          overlapping.push(guest);
          guestIds.add(guest.id);
        }
      }
      if (overlapping.length > 0) groups.set(host.id, overlapping);
    }
    return { parallelGroups: groups, groupedGuestIds: guestIds };
  }, [tasks, scheduleMap]);

  const items: ListItem[] = useMemo(() => {
    const filtered = searchQuery
      ? tasks.filter((t) =>
          t.title.toLowerCase().includes(searchQuery.toLowerCase()),
        )
      : tasks;

    const pending = filtered.filter((t) => t.status === 'pending');
    const scheduled = filtered
      .filter((t) => t.status !== 'pending')
      .sort((a, b) => {
        // Sort by scheduled start time (or task end_at as fallback).
        // Use timestamp comparison instead of string localeCompare to
        // avoid date-boundary sorting issues (#210): localeCompare on
        // ISO strings with different dates can produce wrong order when
        // the strings have different lengths or timezone offsets.
        const sa = scheduleMap.get(a.id)?.start_at ?? a.end_at;
        const sb = scheduleMap.get(b.id)?.start_at ?? b.end_at;
        const ta = new Date(sa).getTime();
        const tb = new Date(sb).getTime();
        return ta - tb;
      });

    // Past completed/skipped tasks — always compute count, only include in list when showPast
    const now = Date.now();
    // #254: 完了/スキップ済みタスクは end_at に関わらず過去セクションへ。
    // fixed タスクは完了後も schedule の end_at が未来になりうるため、
    // status ベースで過去判定しないと upcoming に残り続ける。
    const isPast = (t: TaskRow): boolean => {
      if (t.status === 'completed' || t.status === 'skipped') return true;
      const entry = scheduleMap.get(t.id);
      const end = entry?.end_at ?? t.end_at;
      return new Date(end).getTime() < now;
    };
    const pastAll = scheduled.filter(isPast);
    const past = showPast ? pastAll : [];

    // Upcoming = always exclude past tasks, regardless of showPast
    const upcoming = scheduled.filter((t) => !isPast(t));

    const result: ListItem[] = [];

    // Past section (when revealed) — no date separators, 1 week at a time (#206)
    if (past.length > 0) {
      const weekCutoff = now - pastWeeks * 7 * 24 * 60 * 60 * 1000;
      const pastVisible = past.filter((t) => {
        const entry = scheduleMap.get(t.id);
        const end = entry?.end_at ?? t.end_at;
        return new Date(end).getTime() >= weekCutoff;
      });
      const olderCount = past.length - pastVisible.length;
      if (pastVisible.length > 0 || olderCount > 0) {
        result.push({ type: 'separator', label: '過去' });
      }
      for (const t of pastVisible) {
        if (groupedGuestIds.has(t.id)) continue;
        const entry = scheduleMap.get(t.id);
        const key = dateKey(entry?.start_at ?? t.end_at, serverTz);
        result.push({
          type: 'task',
          task: t,
          scheduleStart: entry?.start_at,
          scheduleEnd: entry?.end_at,
          isDone: t.status === 'completed' || t.status === 'skipped',
          dateKey: key,
        });
      }
      // "Load more" separator if there are older tasks
      if (olderCount > 0) {
        result.push({
          type: 'separator',
          label: `過去をさらに読み込む (${olderCount})`,
        });
      }
    }

    if (pending.length > 0) {
      result.push({ type: 'separator', label: 'pending' });
      for (const t of pending) {
        result.push({
          type: 'task',
          task: t,
          isDone: t.status === 'completed' || t.status === 'skipped',
          dateKey: 'pending',
        });
      }
    }

    let lastDate = '';
    // When searching, render all matching tasks individually — parallel
    // grouping is based on the full task list, so a search-filtered guest
    // could be invisible if its host doesn't match the query.
    const skipGrouping = searchQuery.length > 0;
    for (const t of upcoming) {
      // Skip guests that are part of a parallel group — they're rendered
      // inside the group item alongside their host.
      if (!skipGrouping && groupedGuestIds.has(t.id)) continue;
      const entry = scheduleMap.get(t.id);
      const key = dateKey(entry?.start_at ?? t.end_at, serverTz);
      if (key !== lastDate) {
        result.push({ type: 'separator', label: dateLabel(key, serverTz) });
        lastDate = key;
      }
      // If this task is a host with overlapping guests, render a group
      // (but not when searching — see skipGrouping above).
      const groupGuests = !skipGrouping ? parallelGroups.get(t.id) : undefined;
      if (groupGuests && groupGuests.length > 0) {
        result.push({
          type: 'parallelGroup',
          host: t,
          guests: groupGuests,
          hostScheduleStart: entry?.start_at,
          hostScheduleEnd: entry?.end_at,
          guestScheduleStarts: groupGuests.map(
            (g) => scheduleMap.get(g.id)?.start_at,
          ),
          guestScheduleEnds: groupGuests.map(
            (g) => scheduleMap.get(g.id)?.end_at,
          ),
          dateKey: key,
        });
      } else {
        result.push({
          type: 'task',
          task: t,
          scheduleStart: entry?.start_at,
          scheduleEnd: entry?.end_at,
          isDone: t.status === 'completed' || t.status === 'skipped',
          dateKey: key,
        });
      }
    }

    return result;
  }, [
    tasks,
    scheduleMap,
    searchQuery,
    groupedGuestIds,
    parallelGroups,
    showPast,
    pastWeeks,
    serverTz,
  ]);

  // Count of past tasks (for badge in header, always computed)
  // #254: completed/skipped は end_at に関わらず過去扱い。
  const pastCount = useMemo(() => {
    const now = Date.now();
    return tasks.filter((t) => {
      if (t.status === 'pending') return false;
      if (t.status === 'completed' || t.status === 'skipped') return true;
      const entry = scheduleMap.get(t.id);
      const end = entry?.end_at ?? t.end_at;
      return new Date(end).getTime() < now;
    }).length;
  }, [tasks, scheduleMap]);

  // Marked dates for calendar overlay (dates that have scheduled tasks)
  const markedDates = useMemo(() => {
    const set = new Set<string>();
    for (const t of tasks) {
      if (t.status === 'pending') continue;
      const entry = scheduleMap.get(t.id);
      const key = dateKey(entry?.start_at ?? t.end_at, serverTz);
      set.add(key);
    }
    return set;
  }, [tasks, scheduleMap, serverTz]);

  // Map dateKey → index in items array (for scroll navigation)
  const dateIndexMap = useMemo(() => {
    const m = new Map<string, number>();
    for (let i = 0; i < items.length; i++) {
      const item = items[i];
      if (item.type === 'separator' && item.label !== 'pending') {
        // Reconstruct dateKey from the label — but we stored label, not key.
        // Instead, find the first task after this separator to get its dateKey.
        for (let j = i + 1; j < items.length; j++) {
          const next = items[j];
          if (next.type === 'task' || next.type === 'parallelGroup') {
            m.set(next.dateKey, i);
            break;
          }
        }
      }
    }
    return m;
  }, [items]);

  function scrollToDateKey(key: string) {
    const idx = dateIndexMap.get(key);
    if (idx !== undefined && listRef.current) {
      listRef.current.scrollToIndex({ index: idx, animated: true });
    }
  }

  function scrollByDay(direction: -1 | 1) {
    if (!listRef.current) return;
    // Jump to the next/previous day separator relative to the currently
    // visible top item. A "day" boundary is a separator whose label is a
    // date (not 'pending' / '過去' / load-more).
    // Clamp the ref index to the current list length — the ref is updated
    // asynchronously by onViewableItemsChanged, so it can hold a stale
    // index larger than items.length after a search/refresh shrinks the
    // list. Without this guard, items[i] would be undefined and crash.
    const from = Math.min(visibleTopIndexRef.current, items.length - 1);
    if (direction < 0) {
      for (let i = from - 1; i >= 0; i--) {
        const item = items[i];
        if (item.type === 'separator' && isDaySeparator(item)) {
          listRef.current.scrollToIndex({ index: i, animated: true });
          return;
        }
      }
      // No earlier day separator — go to the very top.
      listRef.current.scrollToOffset({ offset: 0, animated: true });
    } else {
      for (let i = from + 1; i < items.length; i++) {
        const item = items[i];
        if (item.type === 'separator' && isDaySeparator(item)) {
          listRef.current.scrollToIndex({ index: i, animated: true });
          return;
        }
      }
      // No later day separator — scroll to the bottom.
      listRef.current.scrollToEnd({ animated: true });
    }
  }

  function scrollByPage(direction: -1 | 1) {
    if (!listRef.current) return;
    const viewport = listLayoutHeightRef.current;
    if (viewport <= 0) return;
    // Scroll by one viewport, keeping a small overlap so the user keeps
    // some context at the edge.
    const delta = viewport * 0.9 * direction;
    const newOffset = Math.max(0, scrollOffsetRef.current + delta);
    listRef.current.scrollToOffset({ offset: newOffset, animated: true });
  }

  function jumpToDate(date: Date) {
    // Construct the date key in the server-configured timezone so it
    // matches the keys in dateIndexMap (which are built via dateKey with
    // serverTz). Falls back to device-local if serverTz is unavailable.
    const key = dateKey(date.toISOString(), serverTz);
    scrollToDateKey(key);
  }

  function toggleSelection(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function markDone(task: TaskRow) {
    if (!client) return;
    // Pending tasks: swipe-right completes directly (pending → completed).
    // After completion, the task enters the 3-state cycle (scheduled →
    // in_progress → completed → scheduled). Undo restores the original
    // pending status.
    // Scheduled/in_progress/completed use the 3-state cycle (#312).
    const isDone = task.status === 'completed' || task.status === 'skipped';
    const isInProgress = task.status === 'in_progress';
    const isPending = task.status === 'pending';
    const prevStatus = task.status;
    let newStatus: TaskStatus;
    let actionLabel: string;
    let errorLabel: string;
    if (isPending) {
      // Pending tasks: 2-state toggle pending ↔ completed
      newStatus = 'completed';
      actionLabel = 'mark done';
      errorLabel = 'タスクの完了に失敗';
    } else if (isInProgress) {
      newStatus = 'completed';
      actionLabel = 'mark done';
      errorLabel = 'タスクの完了に失敗';
    } else if (isDone) {
      newStatus = 'scheduled';
      actionLabel = 'undone';
      errorLabel = 'タスクの未完了に失敗';
    } else {
      newStatus = 'in_progress';
      actionLabel = 'start';
      errorLabel = 'タスクの開始に失敗';
    }
    try {
      await client.updateTask(task.id, { status: newStatus });
    } catch (e) {
      showError(e, errorLabel);
      return;
    }
    // Dismiss any delivered notifications for this task (#257).
    dismissTaskNotifications(task.id).catch((e) => logError('通知の消去', e));
    if (prevStatus === 'in_progress') {
      dismissInProgressNotification(task.id).catch((e) =>
        logError('通知の消去', e),
      );
    }
    // Post in-progress notification when starting via swipe (#312)
    if (newStatus === 'in_progress' && notifications.inProgress) {
      postInProgressNotification(task).catch((e) => logError('通知の投稿', e));
    }
    undoRedo.push({
      description: `${actionLabel}: ${task.title}`,
      undo: async () => {
        await client.updateTask(task.id, { status: prevStatus });
        if (newStatus === 'in_progress') {
          dismissInProgressNotification(task.id).catch((e) =>
            logError('通知の消去', e),
          );
        }
        if (prevStatus === 'in_progress' && notifications.inProgress) {
          postInProgressNotification(task).catch((e) =>
            logError('通知の投稿', e),
          );
        }
        await refresh();
      },
      redo: async () => {
        await client.updateTask(task.id, { status: newStatus });
        if (newStatus === 'in_progress' && notifications.inProgress) {
          postInProgressNotification(task).catch((e) =>
            logError('通知の投稿', e),
          );
        }
        if (prevStatus === 'in_progress' && newStatus === 'completed') {
          dismissInProgressNotification(task.id).catch((e) =>
            logError('通知の消去', e),
          );
        }
        await refresh();
      },
    });
    await refresh();
  }

  // Cycle the host task of a parallel group through 3 states
  // (scheduled → in_progress → completed → scheduled), matching the
  // single-task card's swipe behavior (#312, #389).
  // When the host transitions to in_progress, only the host is updated.
  // When the host transitions to completed, non-done guests are also
  // completed. When the host transitions back to scheduled, all tasks
  // (host + guests) are reset to scheduled.
  async function markGroupDone(host: TaskRow, guests: TaskRow[]) {
    if (!client) return;
    const hostDone = host.status === 'completed' || host.status === 'skipped';
    const hostInProgress = host.status === 'in_progress';
    const hostPending = host.status === 'pending';
    let newHostStatus: TaskStatus;
    let actionLabel: string;
    let errorLabel: string;
    if (hostPending) {
      newHostStatus = 'completed';
      actionLabel = 'mark done';
      errorLabel = 'タスクの完了に失敗';
    } else if (hostInProgress) {
      newHostStatus = 'completed';
      actionLabel = 'mark done';
      errorLabel = 'タスクの完了に失敗';
    } else if (hostDone) {
      newHostStatus = 'scheduled';
      actionLabel = 'undone';
      errorLabel = 'タスクの未完了に失敗';
    } else {
      // scheduled → in_progress (only host changes)
      newHostStatus = 'in_progress';
      actionLabel = 'start';
      errorLabel = 'タスクの開始に失敗';
    }

    // Determine which tasks to update.
    // - in_progress: only the host
    // - completed: host + non-done guests
    // - scheduled (undone): host + all guests
    const allTasks = [host, ...guests];
    const prevStatuses = new Map(allTasks.map((t) => [t.id, t.status]));
    const toChange: TaskRow[] = [];
    if (newHostStatus === 'in_progress') {
      toChange.push(host);
    } else if (newHostStatus === 'completed') {
      toChange.push(host);
      for (const g of guests) {
        const gDone = g.status === 'completed' || g.status === 'skipped';
        if (!gDone) toChange.push(g);
      }
    } else {
      // scheduled: reset all
      toChange.push(...allTasks);
    }

    const changed: TaskRow[] = [];
    for (const t of toChange) {
      const targetStatus =
        t === host
          ? newHostStatus
          : newHostStatus === 'scheduled'
            ? 'scheduled'
            : 'completed';
      try {
        await client.updateTask(t.id, { status: targetStatus });
        changed.push(t);
        dismissTaskNotifications(t.id).catch((e) => logError('通知の消去', e));
      } catch (e) {
        showError(e, errorLabel);
        if (changed.length > 0) {
          undoRedo.push({
            description: `${actionLabel} group (partial): ${host.title}`,
            undo: async () => {
              for (const ct of changed) {
                const prev = prevStatuses.get(ct.id)!;
                await client.updateTask(ct.id, { status: prev });
              }
              await refresh();
            },
            redo: async () => {
              for (const ct of changed) {
                const target =
                  ct === host
                    ? newHostStatus
                    : newHostStatus === 'scheduled'
                      ? 'scheduled'
                      : 'completed';
                await client.updateTask(ct.id, { status: target });
              }
              await refresh();
            },
          });
        }
        await refresh();
        return;
      }
      if (t.status === 'in_progress') {
        dismissInProgressNotification(t.id).catch((e) =>
          logError('通知の消去', e),
        );
      }
    }
    // Post in-progress notification when starting host via swipe (#312)
    if (newHostStatus === 'in_progress' && notifications.inProgress) {
      postInProgressNotification(host).catch((e) => logError('通知の投稿', e));
    }
    undoRedo.push({
      description: `${actionLabel} group: ${host.title}`,
      undo: async () => {
        for (const t of toChange) {
          const prev = prevStatuses.get(t.id)!;
          await client.updateTask(t.id, { status: prev });
        }
        if (newHostStatus === 'in_progress') {
          dismissInProgressNotification(host.id).catch((e) =>
            logError('通知の消去', e),
          );
        }
        if (
          host.status === 'in_progress' &&
          newHostStatus === 'completed' &&
          notifications.inProgress
        ) {
          postInProgressNotification(host).catch((e) =>
            logError('通知の投稿', e),
          );
        }
        await refresh();
      },
      redo: async () => {
        for (const t of toChange) {
          const target =
            t === host
              ? newHostStatus
              : newHostStatus === 'scheduled'
                ? 'scheduled'
                : 'completed';
          await client.updateTask(t.id, { status: target });
        }
        if (newHostStatus === 'in_progress' && notifications.inProgress) {
          postInProgressNotification(host).catch((e) =>
            logError('通知の投稿', e),
          );
        }
        if (host.status === 'in_progress' && newHostStatus === 'completed') {
          dismissInProgressNotification(host.id).catch((e) =>
            logError('通知の消去', e),
          );
        }
        await refresh();
      },
    });
    await refresh();
  }

  // Delete all tasks in a parallel group (#194).
  // Tracks partial success so an undo entry is pushed even if some deletes
  // fail, and remaps inter-group dependencies on undo (two-pass).
  async function deleteGroup(host: TaskRow, guests: TaskRow[]) {
    if (!client) return;
    const allTasks = [host, ...guests];
    const deleted: TaskRow[] = [];
    for (const t of allTasks) {
      try {
        await client.deleteTask(t.id);
        deleted.push(t);
      } catch (e) {
        showError(e, 'タスクの削除に失敗');
        break;
      }
    }
    if (deleted.length === 0) return;
    // Track the ids assigned by the server when undo recreates the tasks,
    // so redo deletes the recreated (not the stale original) ids.
    const currentIds: string[] = [...deleted.map((t) => t.id)];
    // Track which tasks have been recreated so a retry after partial
    // failure doesn't create duplicates (mirrors deleteSelected pattern).
    const createdIdx = new Set<number>();
    undoRedo.push({
      description: `delete group: ${host.title}`,
      undo: async () => {
        const oldToNew = new Map<string, string>();
        // First pass: create tasks with no deps, build ID mapping.
        for (let i = 0; i < deleted.length; i++) {
          if (createdIdx.has(i)) {
            // Already recreated on a previous (partial) attempt —
            // record the mapping so the dep-remap pass can find it.
            oldToNew.set(deleted[i].id, currentIds[i]);
            continue;
          }
          const t = deleted[i];
          const recreated = await client.createTask({
            title: t.title,
            description: t.description,
            start_at: t.start_at,
            end_at: t.end_at,
            avg_minutes: t.avg_minutes,
            sigma_minutes: t.sigma_minutes,
            depends: [],
            parallelizable: t.parallelizable,
            allows_parallel: t.allows_parallel,
            abandonability: t.abandonability,
            ical_uid: t.ical_uid,
            habit_id: t.habit_id,
            fixed: t.fixed,
          });
          if (t.status !== 'pending') {
            await client.updateTask(recreated.id, { status: t.status });
          }
          currentIds[i] = recreated.id;
          oldToNew.set(t.id, recreated.id);
          createdIdx.add(i);
        }
        // Second pass: remap inter-group dependencies to new IDs.
        for (let i = 0; i < deleted.length; i++) {
          const t = deleted[i];
          const origDeps = parseDepends(t.depends);
          if (origDeps.length === 0) continue;
          const newId = oldToNew.get(t.id);
          if (!newId) continue;
          const remapped = origDeps.map((d) => oldToNew.get(d) ?? d);
          await client.updateTask(newId, { depends: remapped });
        }
        await refresh();
      },
      redo: async () => {
        createdIdx.clear();
        for (const id of currentIds) {
          await client.deleteTask(id);
        }
        await refresh();
      },
    });
    await refresh();
  }

  async function deleteTask(task: TaskRow) {
    if (!client) return;
    try {
      await client.deleteTask(task.id);
    } catch (e) {
      showError(e, 'タスクの削除に失敗');
      return;
    }
    // Track the id assigned by the server when undo recreates the task,
    // so redo deletes the recreated (not the stale original) id.
    let currentId = task.id;
    undoRedo.push({
      description: `delete: ${task.title}`,
      undo: async () => {
        // Re-create with same fields
        const recreated = await client.createTask({
          title: task.title,
          description: task.description,
          start_at: task.start_at,
          end_at: task.end_at,
          avg_minutes: task.avg_minutes,
          sigma_minutes: task.sigma_minutes,
          depends: parseDepends(task.depends),
          parallelizable: task.parallelizable,
          allows_parallel: task.allows_parallel,
          abandonability: task.abandonability,
          ical_uid: task.ical_uid,
          habit_id: task.habit_id,
          fixed: task.fixed,
        });
        // CreateTask does not accept `status`; restore it via update.
        if (task.status !== 'pending') {
          await client.updateTask(recreated.id, { status: task.status });
        }
        currentId = recreated.id;
        await refresh();
      },
      redo: async () => {
        await client.deleteTask(currentId);
        await refresh();
      },
    });
    await refresh();
  }

  // Run an async operation while showing a status label in the top-bar
  // center. The label is cleared when the operation finishes (success or
  // failure).
  async function withStatus<T>(
    label: string,
    fn: () => Promise<T>,
  ): Promise<T> {
    setStatusLabel(label);
    try {
      return await fn();
    } finally {
      setStatusLabel(null);
    }
  }

  async function rescheduleSelected() {
    if (!client) return;
    const pinned = tasks.filter((t) => !selected.has(t.id)).map((t) => t.id);
    const until = new Date();
    until.setDate(until.getDate() + 7);
    try {
      await withStatus('reschedule中', () =>
        client.reschedule({
          mode: 'range',
          from: new Date().toISOString(),
          until: until.toISOString(),
          pinned,
        }),
      );
    } catch (e) {
      showError(e, '再スケジュールに失敗');
      return;
    }
    setSelected(new Set());
    await refresh();
  }

  async function rescheduleOthers() {
    if (!client) return;
    const pinned = Array.from(selected);
    const until = new Date();
    until.setDate(until.getDate() + 7);
    try {
      await withStatus('reschedule中', () =>
        client.reschedule({
          mode: 'range',
          from: new Date().toISOString(),
          until: until.toISOString(),
          pinned,
        }),
      );
    } catch (e) {
      showError(e, '再スケジュールに失敗');
      return;
    }
    setSelected(new Set());
    await refresh();
  }

  async function deleteSelected() {
    if (!client) return;
    const toDelete = tasks.filter((t) => selected.has(t.id));
    if (toDelete.length === 0) return;
    // #242: confirm before batch-deleting tasks.
    const confirmed = await new Promise<boolean>((resolve) => {
      Alert.alert(
        'タスクを削除',
        `${toDelete.length}件のタスクを削除しますか？`,
        [
          {
            text: 'キャンセル',
            style: 'cancel',
            onPress: () => resolve(false),
          },
          {
            text: '削除',
            style: 'destructive',
            onPress: () => resolve(true),
          },
        ],
        { cancelable: true, onDismiss: () => resolve(false) },
      );
    });
    if (!confirmed) return;
    const deleted: TaskRow[] = [];
    let failed = 0;
    for (const task of toDelete) {
      try {
        await client.deleteTask(task.id);
        deleted.push(task);
      } catch (e) {
        failed++;
        logError(`タスク削除 (${task.title})`, e);
      }
    }
    if (failed > 0) {
      showError(`${failed}件の削除に失敗しました`, 'タスクの削除');
    }
    if (deleted.length === 0) return;
    // Track the ids assigned by the server when undo recreates the tasks,
    // so redo deletes the recreated (not the stale original) ids.
    // Push a single grouped undo entry so one undo restores all tasks.
    const currentIds: string[] = [...deleted.map((t) => t.id)];
    // Track which items have been recreated so a retry after partial failure
    // doesn't create duplicates.
    const createdIdx = new Set<number>();
    undoRedo.push({
      description:
        deleted.length === 1
          ? `delete: ${deleted[0].title}`
          : `delete ${deleted.length} tasks`,
      undo: async () => {
        const oldToNew = new Map<string, string>();
        // First pass: create tasks not yet recreated (skip on retry).
        for (let i = 0; i < deleted.length; i++) {
          if (createdIdx.has(i)) {
            // Already recreated on a previous (partial) attempt.
            oldToNew.set(deleted[i].id, currentIds[i]);
            continue;
          }
          const task = deleted[i];
          const recreated = await client.createTask({
            title: task.title,
            description: task.description,
            start_at: task.start_at,
            end_at: task.end_at,
            avg_minutes: task.avg_minutes,
            sigma_minutes: task.sigma_minutes,
            depends: [],
            parallelizable: task.parallelizable,
            allows_parallel: task.allows_parallel,
            abandonability: task.abandonability,
            ical_uid: task.ical_uid,
            habit_id: task.habit_id,
            fixed: task.fixed,
          });
          // CreateTask does not accept `status`; restore it via update.
          if (task.status !== 'pending') {
            await client.updateTask(recreated.id, { status: task.status });
          }
          currentIds[i] = recreated.id;
          oldToNew.set(task.id, recreated.id);
          createdIdx.add(i);
        }
        // Second pass: remap depends to new IDs for deps within the deleted set.
        for (let i = 0; i < deleted.length; i++) {
          const task = deleted[i];
          const origDeps = parseDepends(task.depends);
          if (origDeps.length === 0) continue;
          const newId = oldToNew.get(task.id)!;
          const remapped = origDeps.map((d) => oldToNew.get(d) ?? d);
          await client.updateTask(newId, { depends: remapped });
        }
        await refresh();
      },
      redo: async () => {
        createdIdx.clear();
        for (const id of currentIds) {
          await client.deleteTask(id);
        }
        await refresh();
      },
    });
    setSelected(new Set());
    await refresh();
  }

  function createDependent() {
    const deps = Array.from(selected);
    setSelected(new Set());
    router.push({
      pathname: '/task/add',
      params: { deps: JSON.stringify(deps) },
    });
  }

  async function setStatusSelected(newStatus: TaskStatus) {
    if (!client) return;
    const toUpdate = tasks.filter((t) => selected.has(t.id));
    if (toUpdate.length === 0) return;
    const prevStatuses = new Map(toUpdate.map((t) => [t.id, t.status]));
    const changed: TaskRow[] = [];
    let failed = 0;
    for (const task of toUpdate) {
      if (task.status === newStatus) continue;
      try {
        await client.updateTask(task.id, { status: newStatus });
        changed.push(task);
        // Dismiss any delivered notifications for this task (#257).
        dismissTaskNotifications(task.id).catch((e) =>
          logError('通知の消去', e),
        );
        if (task.status === 'in_progress') {
          dismissInProgressNotification(task.id).catch((e) =>
            logError('通知の消去', e),
          );
        }
      } catch (e) {
        failed++;
        logError(`ステータス変更 (${task.title})`, e);
      }
    }
    if (failed > 0) {
      showError(
        `${failed}件のステータス変更に失敗しました`,
        'ステータスの一括設定',
      );
    }
    if (changed.length === 0) {
      await refresh();
      return;
    }
    undoRedo.push({
      description:
        changed.length === 1
          ? `set status ${newStatus}: ${changed[0].title}`
          : `set status ${newStatus} on ${changed.length} tasks`,
      undo: async () => {
        for (const t of changed) {
          const prev = prevStatuses.get(t.id)!;
          await client.updateTask(t.id, { status: prev });
        }
        await refresh();
      },
      redo: async () => {
        for (const t of changed) {
          await client.updateTask(t.id, { status: newStatus });
        }
        await refresh();
      },
    });
    setSelected(new Set());
    await refresh();
  }

  // ── Bottom-sheet preview handlers (AddButton drag → TaskAddSheet) ──
  function handleAddDragStart() {
    // Cancel any pending unmount so a quick second drag can't have the
    // sheet yanked out from under the user mid-gesture.
    cancelUnmount();
    // Reset to current hidden position so a dimension change (e.g. rotation)
    // doesn't leave sheetY at a stale value that would flash the sheet.
    sheetY.value = screenHeight;
    setSheetMounted(true);
    setSheetOpen(false);
  }

  function handleAddDragEnd(committed: boolean) {
    if (committed) {
      cancelUnmount();
      sheetY.value = withTiming(0, { duration: 200 });
      setSheetOpen(true);
    } else {
      sheetY.value = withTiming(screenHeight, { duration: 200 });
      setSheetOpen(false);
      // Unmount after the close animation finishes.
      scheduleUnmount();
    }
  }

  function closeAddSheet() {
    sheetY.value = withTiming(screenHeight, { duration: 200 });
    setSheetOpen(false);
    scheduleUnmount();
    // Refresh the task list so a newly created task appears immediately.
    refresh();
  }

  function renderItem(item: ListItem) {
    if (item.type === 'separator') {
      // "Load more past" separator is tappable (#206)
      if (item.label.startsWith('過去をさらに読み込む')) {
        return (
          <Pressable
            style={styles.separator}
            onPress={() => {
              haptic.light();
              setPastWeeks((w) => w + 1);
            }}
          >
            <View style={styles.separatorBar} />
            <Text style={[styles.separatorText, { color: BRAND_COLOR }]}>
              {item.label}
            </Text>
            <View style={styles.separatorBar} />
          </Pressable>
        );
      }
      return (
        <View style={styles.separator}>
          <View style={styles.separatorBar} />
          <Text style={styles.separatorText}>{item.label}</Text>
          <View style={styles.separatorBar} />
        </View>
      );
    }
    if (item.type === 'parallelGroup') {
      const allIds = [item.host.id, ...item.guests.map((g) => g.id)];
      const isSelected = allIds.some((id) => selected.has(id));
      // Toggle all group IDs atomically: derive add-vs-remove from the
      // latest state inside the updater to avoid stale closure bugs.
      function toggleGroupSelection() {
        setSelected((prev) => {
          const next = new Set(prev);
          const allSel = allIds.every((id) => prev.has(id));
          if (allSel) {
            for (const id of allIds) next.delete(id);
          } else {
            for (const id of allIds) next.add(id);
          }
          return next;
        });
      }
      return (
        <ParallelGroupCard
          host={item.host}
          guests={item.guests}
          hostScheduleStart={item.hostScheduleStart}
          hostScheduleEnd={item.hostScheduleEnd}
          guestScheduleStarts={item.guestScheduleStarts}
          guestScheduleEnds={item.guestScheduleEnds}
          selected={isSelected}
          habitDisplayIdMap={habitDisplayIdMap}
          onHostPress={() => {
            if (selected.size > 0) {
              toggleGroupSelection();
            } else {
              router.push(`/task/${item.host.id}`);
            }
          }}
          onGuestPress={(idx) => {
            if (selected.size > 0) {
              toggleGroupSelection();
            } else {
              router.push(`/task/${item.guests[idx].id}`);
            }
          }}
          onLongPress={toggleGroupSelection}
          onDone={() => markGroupDone(item.host, item.guests)}
          onDelete={() => deleteGroup(item.host, item.guests)}
        />
      );
    }
    const isSelected = selected.has(item.task.id);
    return (
      <TaskCard
        task={item.task}
        scheduleStart={item.scheduleStart}
        scheduleEnd={item.scheduleEnd}
        isDone={item.isDone}
        selected={isSelected}
        habitDisplayId={
          item.task.habit_id
            ? habitDisplayIdMap.get(item.task.habit_id)
            : undefined
        }
        onPress={() => {
          if (selected.size > 0) {
            toggleSelection(item.task.id);
          } else {
            router.push(`/task/${item.task.id}`);
          }
        }}
        onLongPress={() => toggleSelection(item.task.id)}
        onDone={() => markDone(item.task)}
        onDelete={() => deleteTask(item.task)}
      />
    );
  }

  if (view === 'graph') {
    return (
      <GraphWrapper
        client={client}
        onBack={() => setView('task')}
        onTaskPress={(taskId) => router.push(`/task/${taskId}`)}
        viewChanger={viewChanger}
      />
    );
  }

  if (view === 'habit') {
    return <HabitWrapper client={client} viewChanger={viewChanger} />;
  }

  return (
    <View style={[styles.container, { backgroundColor: colors.white }]}>
      {/* Top bar */}
      <View style={[styles.topBar, { paddingTop: 8 + insets.top }]}>
        <ContextMenu
          hasSelection={selected.size > 0}
          onSettings={() => router.push('/settings')}
          onUndo={() =>
            undoRedo
              .undo()
              .then(refresh)
              .catch((e) => showError(e, 'アンドゥに失敗'))
          }
          onRedo={() =>
            undoRedo
              .redo()
              .then(refresh)
              .catch((e) => showError(e, 'リドゥに失敗'))
          }
          onSelectAll={() =>
            setSelected(
              new Set(
                items.flatMap((it) =>
                  it.type === 'task'
                    ? [it.task.id]
                    : it.type === 'parallelGroup'
                      ? [it.host.id, ...it.guests.map((g) => g.id)]
                      : [],
                ),
              ),
            )
          }
          onRescheduleSelected={rescheduleSelected}
          onRescheduleOthers={rescheduleOthers}
          onDeleteSelected={deleteSelected}
          onCreateDependent={createDependent}
          onSetStatusSelected={setStatusSelected}
          onClearSelection={() => setSelected(new Set())}
        />
        <Pressable
          style={({ pressed }) => [
            styles.topButton,
            pressed && styles.topButtonPressed,
          ]}
          onPress={() => {
            haptic.light();
            setSearchOpen(!searchOpen);
          }}
        >
          <Ionicons name="search-outline" size={22} color={BRAND_COLOR} />
        </Pressable>
        {searchOpen && (
          <TextInput
            style={[
              styles.searchInput,
              { borderColor: colors.separator, color: colors.black },
            ]}
            value={searchQuery}
            onChangeText={setSearchQuery}
            placeholder="検索..."
            placeholderTextColor={colors.grayLight}
            autoFocus
          />
        )}
        {/* Flex spacer — keeps the refresh button right-aligned when search
            is closed. The status label is absolutely positioned inside so
            it stays centered regardless of left/right button widths (#304). */}
        <View style={styles.topBarCenter}>
          {statusLabel && (
            <View style={styles.statusLabelAbsolute} pointerEvents="none">
              <View style={styles.statusPill}>
                <ActivityIndicator size="small" color={BRAND_COLOR} />
                <Text style={styles.statusText}>{statusLabel}</Text>
              </View>
            </View>
          )}
        </View>
        <Pressable
          style={({ pressed }) => [
            styles.topButton,
            pressed && styles.topButtonPressed,
          ]}
          onPress={async () => {
            if (!client) return;
            haptic.medium();
            try {
              await withStatus('スケジュール生成中', () =>
                client.generateSchedule({}),
              );
              // Trigger Google Calendar sync (no-op if not configured)
              await withStatus('GCal同期中', () =>
                client
                  .triggerSync()
                  .catch((e) => logError('Google Calendar同期', e)),
              );
            } catch (e) {
              showError(e, 'スケジュール生成に失敗');
            }
            await refresh();
          }}
        >
          <Ionicons name="refresh" size={22} color={BRAND_COLOR} />
        </Pressable>
      </View>

      {/* Task list */}
      <FlatList
        ref={listRef}
        data={items}
        keyExtractor={(item, i) =>
          item.type === 'separator'
            ? `sep-${i}`
            : item.type === 'parallelGroup'
              ? `group-${item.host.id}`
              : `task-${item.task.id}`
        }
        renderItem={({ item }) => renderItem(item)}
        ListHeaderComponent={
          pastCount > 0 ? (
            <Pressable style={styles.pastToggle} onPress={togglePast}>
              <Reanimated.View style={chevronStyle}>
                <Ionicons name="chevron-down" size={16} color={BRAND_COLOR} />
              </Reanimated.View>
              <Text style={styles.pastToggleText}>
                {showPast ? '過去を隠す' : '過去を表示'}
              </Text>
              <View
                style={[styles.pastBadge, { backgroundColor: BRAND_COLOR }]}
              >
                <Text style={styles.pastBadgeText}>{pastCount}</Text>
              </View>
            </Pressable>
          ) : null
        }
        refreshControl={
          <RefreshControl refreshing={refreshing} onRefresh={refresh} />
        }
        onScroll={(e) => {
          scrollOffsetRef.current = e.nativeEvent.contentOffset.y;
        }}
        onScrollBeginDrag={showNavButtons}
        onScrollEndDrag={scheduleHideNavButtons}
        onMomentumScrollBegin={showNavButtons}
        onMomentumScrollEnd={scheduleHideNavButtons}
        scrollEventThrottle={16}
        onLayout={(e) => {
          listLayoutHeightRef.current = e.nativeEvent.layout.height;
        }}
        onViewableItemsChanged={handleViewableItemsChanged}
        viewabilityConfig={VIEWABILITY_CONFIG}
        onScrollToIndexFailed={({ index, averageItemLength }) => {
          // Fallback: scroll to approximate offset
          listRef.current?.scrollToOffset({
            offset: index * averageItemLength,
            animated: true,
          });
        }}
        contentContainerStyle={[
          styles.listContent,
          { paddingBottom: 100 + insets.bottom },
        ]}
      />

      {/* Bottom bar */}
      <View style={[styles.bottomBar, { paddingBottom: 16 + insets.bottom }]}>
        <AddButton
          onSlideUp={() => {}}
          sheetY={sheetY}
          screenHeight={screenHeight}
          onDragStart={handleAddDragStart}
          onDragEnd={handleAddDragEnd}
        />
        <Pressable
          style={[styles.startDoneButton, { bottom: 16 + insets.bottom }]}
          onPress={async () => {
            // Start next task — mark as in_progress.
            // #256: pick the chronologically next scheduled task (earliest
            // start time) so the user can start it early (前倒し), not just
            // the most recently created one. Fall back to pending tasks.
            const scheduled = tasks
              .filter((t) => t.status === 'scheduled')
              .sort((a, b) => {
                const sa = scheduleMap.get(a.id)?.start_at ?? a.end_at;
                const sb = scheduleMap.get(b.id)?.start_at ?? b.end_at;
                return new Date(sa).getTime() - new Date(sb).getTime();
              });
            const next =
              scheduled[0] ?? tasks.find((t) => t.status === 'pending');
            if (next) {
              haptic.medium();
              if (client && next.status !== 'in_progress') {
                try {
                  await client.updateTask(next.id, { status: 'in_progress' });
                  // Dismiss any delivered start reminder notifications (#257)
                  dismissTaskNotifications(next.id).catch((e) =>
                    logError('通知の消去', e),
                  );
                  // Post in-progress notification with DONE/CANCEL actions
                  if (notifications.inProgress) {
                    postInProgressNotification(next).catch((e) =>
                      logError('通知の投稿', e),
                    );
                  }
                } catch (e) {
                  showError(e, 'タスクの開始に失敗');
                  return;
                }
              }
              router.push(`/task/${next.id}`);
            }
          }}
        >
          <Ionicons name="play" size={24} color={COLORS.white} />
        </Pressable>
      </View>

      {/* Floating navigation — visible only while scrolling (#308) */}
      <Reanimated.View
        style={[
          { position: 'absolute', top: 0, bottom: 0, left: 0, right: 0 },
          navButtonsStyle,
        ]}
        pointerEvents={navButtonsVisible ? 'box-none' : 'none'}
      >
        <NavigationButtons
          onScrollUpByDay={() => {
            showNavButtons();
            scrollByDay(-1);
            scheduleHideNavButtons();
          }}
          onScrollUpByPage={() => {
            showNavButtons();
            scrollByPage(-1);
            scheduleHideNavButtons();
          }}
          onScrollDownByDay={() => {
            showNavButtons();
            scrollByDay(1);
            scheduleHideNavButtons();
          }}
          onScrollDownByPage={() => {
            showNavButtons();
            scrollByPage(1);
            scheduleHideNavButtons();
          }}
          onJumpToDate={(date) => {
            showNavButtons();
            jumpToDate(date);
            scheduleHideNavButtons();
          }}
          markedDates={markedDates}
        />
      </Reanimated.View>

      {/* View changer */}
      <ViewChanger current={view} onChange={setView} />

      {/* Task-add bottom-sheet preview (revealed by dragging the add button) */}
      {sheetMounted && (
        <TaskAddSheet
          sheetY={sheetY}
          screenHeight={screenHeight}
          open={sheetOpen}
          onClose={closeAddSheet}
        />
      )}
    </View>
  );
}

// Placeholder wrappers for graph and habit views within home
function GraphWrapper({
  client,
  onBack,
  viewChanger,
  onTaskPress,
}: {
  client: TakusuClient | null;
  onBack: () => void;
  viewChanger: React.ReactNode;
  onTaskPress: (taskId: string) => void;
}) {
  // Lazy load to avoid circular deps
  const { GraphView } = require('@/src/views/GraphView');
  return (
    <View style={{ flex: 1 }}>
      <GraphView client={client} onBack={onBack} onTaskPress={onTaskPress} />
      {viewChanger}
    </View>
  );
}

function HabitWrapper({
  client,
  viewChanger,
}: {
  client: TakusuClient | null;
  viewChanger: React.ReactNode;
}) {
  const { HabitView } = require('@/src/views/HabitView');
  return (
    <View style={{ flex: 1 }}>
      <HabitView client={client} />
      {viewChanger}
    </View>
  );
}

// Need to import TextInput
import { TextInput } from 'react-native';

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  pastToggle: {
    flexDirection: 'row',
    paddingVertical: 10,
    alignItems: 'center',
    justifyContent: 'center',
    gap: 6,
  },
  pastToggleText: {
    fontSize: 13,
    color: BRAND_COLOR,
    fontWeight: '500',
  },
  pastBadge: {
    minWidth: 20,
    height: 20,
    borderRadius: 10,
    paddingHorizontal: 6,
    alignItems: 'center',
    justifyContent: 'center',
  },
  pastBadgeText: {
    fontSize: 11,
    color: COLORS.white,
    fontWeight: '600',
  },
  topBar: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 8,
    paddingBottom: 8,
    gap: 4,
  },
  topButton: {
    width: 40,
    height: 40,
    borderRadius: 20,
    alignItems: 'center',
    justifyContent: 'center',
  },
  topBarCenter: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
  },
  statusLabelAbsolute: {
    position: 'absolute',
    left: 0,
    right: 0,
    top: 0,
    bottom: 0,
    alignItems: 'center',
    justifyContent: 'center',
  },
  statusPill: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 14,
    backgroundColor: 'rgba(114,97,163,0.1)',
  },
  statusText: {
    fontSize: 12,
    color: BRAND_COLOR,
    fontWeight: '500',
  },
  topButtonPressed: {
    backgroundColor: 'rgba(114,97,163,0.1)',
  },
  topButtonText: {
    fontSize: 20,
  },
  searchInput: {
    flex: 1,
    height: 40,
    borderWidth: 1,
    borderColor: COLORS.separator,
    borderRadius: 12,
    paddingHorizontal: 16,
    paddingVertical: 0,
    fontSize: 16,
  },
  listContent: {
    paddingBottom: 100,
  },
  separator: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 16,
    paddingVertical: 8,
    gap: 8,
  },
  separatorBar: {
    flex: 1,
    height: 1,
    backgroundColor: COLORS.separator,
  },
  separatorText: {
    fontSize: 12,
    color: COLORS.gray,
    fontWeight: '500',
  },
  bottomBar: {
    position: 'absolute',
    bottom: 0,
    left: 0,
    right: 0,
    flexDirection: 'row',
    justifyContent: 'center',
    alignItems: 'center',
    paddingVertical: 16,
    paddingHorizontal: 24,
    gap: 16,
  },
  addButton: {
    width: 56,
    height: 56,
    borderRadius: 28,
    backgroundColor: BRAND_COLOR,
    alignItems: 'center',
    justifyContent: 'center',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.3,
    shadowRadius: 4,
    elevation: 4,
  },
  addButtonText: {
    fontSize: 28,
    color: COLORS.white,
    fontWeight: '300',
  },
  startDoneButton: {
    position: 'absolute',
    right: 24,
    bottom: 16,
    width: 48,
    height: 48,
    borderRadius: 24,
    backgroundColor: COLORS.green,
    alignItems: 'center',
    justifyContent: 'center',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.3,
    shadowRadius: 4,
    elevation: 4,
  },
  startDoneText: {
    fontSize: 20,
    color: COLORS.white,
  },
} as Record<string, ViewStyle>);
