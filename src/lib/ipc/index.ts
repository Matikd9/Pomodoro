// Typed wrappers around Tauri invoke() / listen() or Remote HTTP / WebSocket backend.
// All backend communication goes through this module.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { open as dialogOpen } from '@tauri-apps/plugin-dialog';
import { isTauri } from '$lib/utils/platform';
import type {
  TimerState,
  Settings,
  Theme,
  CustomAudioInfo,
  DetailedStats,
  DailyStats,
  CalendarWeekStats,
  HeatmapStats,
  UpdateInfo,
  TaskItem,
  TaskStatsSummary,
  ObsidianExportResult,
  PresetItem,
} from '$lib/types';

// --- Remote Server Configuration ---

const REMOTE_URL_KEY = 'pomotroid_remote_url';
export const DEFAULT_REMOTE_URL = 'http://100.64.60.57:8085';

export function getRemoteServerUrl(): string | null {
  if (typeof window === 'undefined') return null;
  const configured = localStorage.getItem(REMOTE_URL_KEY);
  if (configured !== null) {
    const trimmed = configured.trim();
    if (!trimmed) return null; // Explicitly set to empty by user for local mode
    return trimmed.replace(/\/+$/, '');
  }
  // Default to user's Tailscale server
  return DEFAULT_REMOTE_URL;
}

export function setRemoteServerUrl(url: string | null): void {
  if (typeof window === 'undefined') return;
  if (!url || !url.trim()) {
    localStorage.removeItem(REMOTE_URL_KEY);
  } else {
    localStorage.setItem(REMOTE_URL_KEY, url.trim().replace(/\/+$/, ''));
  }
  if (activeWs) {
    activeWs.close();
    activeWs = null;
  }
  if (isRemoteMode()) {
    ensureWebSocket();
  }
}

export function isRemoteMode(): boolean {
  return getRemoteServerUrl() !== null;
}

// --- Remote HTTP Fetch Helper ---

async function remoteFetch<T>(path: string, options?: RequestInit): Promise<T> {
  const base = getRemoteServerUrl() ?? (typeof window !== 'undefined' ? window.location.origin : '');
  const res = await fetch(`${base}${path}`, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...options?.headers,
    },
  });
  if (!res.ok) {
    throw new Error(`Remote API request to ${path} failed with status ${res.status}`);
  }
  const contentType = res.headers.get('content-type') || '';
  if (!contentType.includes('application/json')) {
    throw new Error(`Remote API request to ${path} returned non-JSON content: ${contentType}`);
  }
  return res.json() as Promise<T>;
}

// --- WebSocket Event Handling for Remote / Web Mode ---

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type WsCallback = (payload: any) => void;
const wsListeners = new Map<string, Set<WsCallback>>();

let activeWs: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

function getWsUrl(): string {
  const remote = getRemoteServerUrl();
  if (remote) {
    return remote.replace(/^http/, 'ws') + '/ws';
  }
  if (typeof window !== 'undefined') {
    const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    return `${proto}//${window.location.host}/ws`;
  }
  return 'ws://localhost:8085/ws';
}

function playWebSound(type: string) {
  if (typeof window === 'undefined' || isTauri) return;
  try {
    let soundFile = '/audio/alert-work.mp3';
    if (type === 'short-break') soundFile = '/audio/alert-short-break.mp3';
    else if (type === 'long-break') soundFile = '/audio/alert-long-break.mp3';
    const audio = new Audio(soundFile);
    audio.play().catch(() => {});
  } catch {
    // Ignore audio autoplay restrictions
  }
}

function ensureWebSocket() {
  if (!isRemoteMode()) return;
  if (
    activeWs &&
    (activeWs.readyState === WebSocket.OPEN || activeWs.readyState === WebSocket.CONNECTING)
  ) {
    return;
  }
  try {
    const url = getWsUrl();
    activeWs = new WebSocket(url);

    activeWs.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        const { type, payload } = data;
        if (type === 'tick') emitWs('timer:tick', payload);
        else if (type === 'paused') emitWs('timer:paused', payload);
        else if (type === 'resumed') emitWs('timer:resumed', payload);
        else if (type === 'roundChange') {
          emitWs('timer:round-change', payload);
          playWebSound(payload.round_type);
        } else if (type === 'reset') emitWs('timer:reset', payload);
        else if (type === 'settingsChanged') emitWs('settings:changed', payload);
        else if (type === 'sessionsCleared') emitWs('sessions:cleared', undefined);
        else if (type === 'presetsChanged') emitWs('presets:changed', payload);
      } catch (e) {
        console.error('Failed to parse WS message', e);
      }
    };

    activeWs.onclose = () => {
      activeWs = null;
      if (isRemoteMode()) {
        if (reconnectTimer) clearTimeout(reconnectTimer);
        reconnectTimer = setTimeout(ensureWebSocket, 2500);
      }
    };

    activeWs.onerror = () => {
      activeWs?.close();
    };
  } catch {
    if (reconnectTimer) clearTimeout(reconnectTimer);
    reconnectTimer = setTimeout(ensureWebSocket, 3000);
  }
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function emitWs(event: string, payload: any) {
  const set = wsListeners.get(event);
  if (set) {
    for (const cb of set) {
      try {
        cb(payload);
      } catch (err) {
        console.error(`Error in listener for ${event}:`, err);
      }
    }
  }
}

function addWsListener(event: string, cb: WsCallback): UnlistenFn {
  ensureWebSocket();
  if (!wsListeners.has(event)) {
    wsListeners.set(event, new Set());
  }
  wsListeners.get(event)!.add(cb);
  return () => {
    wsListeners.get(event)?.delete(cb);
  };
}

// --- Timer commands ---

export const timerToggle = async () => {
  if (isRemoteMode()) {
    await remoteFetch<void>('/api/timer/toggle', { method: 'POST' });
    return;
  }
  return invoke<void>('timer_toggle');
};

export const timerReset = async () => {
  if (isRemoteMode()) {
    await remoteFetch<void>('/api/timer/reset', { method: 'POST' });
    return;
  }
  return invoke<void>('timer_reset');
};

export const timerRestartRound = async () => {
  if (isRemoteMode()) {
    await remoteFetch<void>('/api/timer/restart', { method: 'POST' });
    return;
  }
  return invoke<void>('timer_restart_round');
};

export const timerSkip = async () => {
  if (isRemoteMode()) {
    await remoteFetch<void>('/api/timer/skip', { method: 'POST' });
    return;
  }
  return invoke<void>('timer_skip');
};

export const getTimerState = async () => {
  if (isRemoteMode()) {
    return remoteFetch<TimerState>('/api/state');
  }
  return invoke<TimerState>('timer_get_state');
};

export const timerSetTask = async (task: string) => {
  if (isRemoteMode()) {
    return remoteFetch<TimerState>('/api/timer/task', {
      method: 'POST',
      body: JSON.stringify({ task }),
    });
  }
  return invoke<TimerState>('timer_set_task', { task });
};

// --- Tasks commands ---

export function normalizeTask(item: unknown): TaskItem {
  if (typeof item === 'string') {
    return { name: item, completed: false, deleted: false };
  }
  if (item && typeof item === 'object') {
    const obj = item as Record<string, unknown>;
    const name = typeof obj.name === 'string' ? obj.name : String(obj.name ?? '');
    return {
      name: name || 'General',
      completed: Boolean(obj.completed),
      deleted: Boolean(obj.deleted),
    };
  }
  return { name: String(item ?? 'General'), completed: false, deleted: false };
}

export function normalizeTaskStatsSummary(item: unknown): TaskStatsSummary {
  if (!item || typeof item !== 'object') {
    return {
      name: 'General',
      completed: false,
      deleted: false,
      all_time_secs: 0,
      all_time_rounds: 0,
      month_secs: 0,
      month_rounds: 0,
      week_secs: 0,
      week_rounds: 0,
      today_secs: 0,
      today_rounds: 0,
    };
  }
  const obj = item as Record<string, unknown>;
  const name = typeof obj.name === 'string' ? obj.name : String(obj.name ?? 'General');
  return {
    name: name || 'General',
    completed: Boolean(obj.completed),
    deleted: Boolean(obj.deleted),
    all_time_secs: Number(obj.all_time_secs ?? obj.allTimeSecs ?? 0),
    all_time_rounds: Number(obj.all_time_rounds ?? obj.allTimeRounds ?? 0),
    month_secs: Number(obj.month_secs ?? obj.monthSecs ?? 0),
    month_rounds: Number(obj.month_rounds ?? obj.monthRounds ?? 0),
    week_secs: Number(obj.week_secs ?? obj.weekSecs ?? 0),
    week_rounds: Number(obj.week_rounds ?? obj.weekRounds ?? 0),
    today_secs: Number(obj.today_secs ?? obj.todaySecs ?? 0),
    today_rounds: Number(obj.today_rounds ?? obj.todayRounds ?? 0),
  };
}

export const tasksList = async (): Promise<TaskItem[]> => {
  let raw: unknown[];
  if (isRemoteMode()) {
    try {
      raw = await remoteFetch<unknown[]>('/api/tasks');
    } catch (e) {
      console.warn('Failed to fetch tasks from remote:', e);
      if (isTauri) {
        raw = await invoke<unknown[]>('tasks_list');
      } else {
        throw e;
      }
    }
  } else {
    raw = await invoke<unknown[]>('tasks_list');
  }
  const list = (Array.isArray(raw) ? raw : []).map(normalizeTask);
  if (!list.some((t) => t.name === 'General')) {
    list.unshift({ name: 'General', completed: false, deleted: false });
  }
  return list;
};

export const tasksGetSummary = async (): Promise<TaskStatsSummary[]> => {
  if (isRemoteMode()) {
    try {
      const res = await remoteFetch<unknown[]>('/api/tasks/summary');
      if (Array.isArray(res) && res.length > 0) {
        return res.map(normalizeTaskStatsSummary);
      }
    } catch {
      // Remote server does not support /api/tasks/summary yet, calculate robustly from available endpoints
    }

    try {
      const [rawTasks, detailed, currentWeek, ...pastWeeks] = await Promise.all([
        tasksList().catch(() => []),
        statsGetDetailed().catch(() => null),
        statsGetWeeklyByOffset(0).catch(() => null),
        statsGetWeeklyByOffset(-1).catch(() => null),
        statsGetWeeklyByOffset(-2).catch(() => null),
        statsGetWeeklyByOffset(-3).catch(() => null),
        statsGetWeeklyByOffset(-4).catch(() => null),
        statsGetWeeklyByOffset(-5).catch(() => null),
        statsGetWeeklyByOffset(-6).catch(() => null),
        statsGetWeeklyByOffset(-7).catch(() => null),
        statsGetWeeklyByOffset(-8).catch(() => null),
      ]);

      const allWeeks = [currentWeek, ...pastWeeks].filter(Boolean) as CalendarWeekStats[];
      const summaryMap = new Map<string, TaskStatsSummary>();

      const ensureTask = (name: string, completed = false, deleted = false): TaskStatsSummary => {
        const clean = name.trim() || 'General';
        let existing = summaryMap.get(clean);
        if (!existing) {
          existing = {
            name: clean,
            completed,
            deleted,
            all_time_secs: 0,
            all_time_rounds: 0,
            month_secs: 0,
            month_rounds: 0,
            week_secs: 0,
            week_rounds: 0,
            today_secs: 0,
            today_rounds: 0,
          };
          summaryMap.set(clean, existing);
        } else {
          if (completed) existing.completed = true;
          if (deleted) existing.deleted = true;
        }
        return existing;
      };

      // Always ensure 'General' exists
      ensureTask('General');

      // Initialize from tasks list
      if (Array.isArray(rawTasks)) {
        for (const t of rawTasks) {
          ensureTask(t.name, t.completed, t.deleted ?? false);
        }
      }

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const extractStat = (item: any): { name: string; secs: number; rounds: number } => {
        const name = (item.task_name ?? item.task ?? item.name ?? 'General').trim() || 'General';
        const secs = Number(item.focus_secs ?? item.total_secs ?? item.duration_secs ?? 0);
        const rounds = Number(item.rounds ?? item.total_rounds ?? item.completed_rounds ?? 0);
        return { name, secs, rounds };
      };

      // 1. Accumulate Today's sessions
      if (detailed?.today?.task_breakdown && Array.isArray(detailed.today.task_breakdown)) {
        for (const tb of detailed.today.task_breakdown) {
          const { name, secs, rounds } = extractStat(tb);
          const task = ensureTask(name);
          task.today_secs += secs;
          task.today_rounds += rounds;
        }
      }

      // 2. Accumulate Calendar Weeks (week 0 is this week, others contribute to month / all_time)
      const now = new Date();
      const currentYearMonth = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}`;

      allWeeks.forEach((weekObj, index) => {
        if (!weekObj || !Array.isArray(weekObj.tasks)) return;
        const isThisWeek = index === 0;
        const isThisMonth =
          (typeof weekObj.start_date === 'string' && weekObj.start_date.startsWith(currentYearMonth)) ||
          (typeof weekObj.end_date === 'string' && weekObj.end_date.startsWith(currentYearMonth));

        for (const t of weekObj.tasks) {
          const { name, secs, rounds } = extractStat(t);
          const task = ensureTask(name);

          task.all_time_secs += secs;
          task.all_time_rounds += rounds;

          if (isThisMonth) {
            task.month_secs += secs;
            task.month_rounds += rounds;
          }

          if (isThisWeek) {
            task.week_secs += secs;
            task.week_rounds += rounds;
          }
        }
      });

      // 3. Fallback check: if weekObj.tasks missed any tasks present in detailed.week_tasks
      if (detailed?.week_tasks && Array.isArray(detailed.week_tasks)) {
        for (const wt of detailed.week_tasks) {
          const { name, secs, rounds } = extractStat(wt);
          const task = ensureTask(name);
          if (task.week_secs === 0 && secs > 0) {
            task.week_secs = secs;
            task.week_rounds = rounds;
          }
          if (task.all_time_secs === 0 && secs > 0) {
            task.all_time_secs = secs;
            task.all_time_rounds = rounds;
          }
          if (task.month_secs === 0 && secs > 0) {
            task.month_secs = secs;
            task.month_rounds = rounds;
          }
        }
      }

      // 4. Ensure today's hours are included in all_time if all_time was otherwise 0
      for (const task of summaryMap.values()) {
        if (task.all_time_secs < task.week_secs) {
          task.all_time_secs = task.week_secs;
          task.all_time_rounds = task.week_rounds;
        }
        if (task.all_time_secs < task.today_secs) {
          task.all_time_secs = task.today_secs;
          task.all_time_rounds = task.today_rounds;
        }
        if (task.month_secs < task.week_secs) {
          task.month_secs = task.week_secs;
          task.month_rounds = task.week_rounds;
        }
      }

      const list = Array.from(summaryMap.values());
      list.sort((a, b) => {
        if (a.name === 'General') return -1;
        if (b.name === 'General') return 1;
        return b.all_time_secs - a.all_time_secs;
      });
      return list;
    } catch (fallbackErr) {
      console.warn('Fallback remote task summary computation failed:', fallbackErr);
    }

    if (isTauri) {
      const raw = await invoke<unknown[]>('tasks_get_summary');
      return (Array.isArray(raw) ? raw : []).map(normalizeTaskStatsSummary);
    }
    return [];
  }

  const raw = await invoke<unknown[]>('tasks_get_summary');
  return (Array.isArray(raw) ? raw : []).map(normalizeTaskStatsSummary);
};

export const tasksCreate = async (name: string): Promise<TaskItem[]> => {
  let raw: unknown[];
  if (isRemoteMode()) {
    try {
      raw = await remoteFetch<unknown[]>('/api/tasks', {
        method: 'POST',
        body: JSON.stringify({ name }),
      });
    } catch (e) {
      if (isTauri) {
        raw = await invoke<unknown[]>('tasks_create', { name });
      } else {
        throw e;
      }
    }
  } else {
    raw = await invoke<unknown[]>('tasks_create', { name });
  }
  return (Array.isArray(raw) ? raw : []).map(normalizeTask);
};

export const tasksRename = async (oldName: string, newName: string): Promise<TaskStatsSummary[]> => {
  if (isRemoteMode()) {
    try {
      const res = await remoteFetch<unknown[]>('/api/tasks/rename', {
        method: 'POST',
        body: JSON.stringify({ old_name: oldName, new_name: newName }),
      });
      return (Array.isArray(res) ? res : []).map(normalizeTaskStatsSummary);
    } catch (e) {
      console.warn('Remote /api/tasks/rename failed or not supported yet, falling back:', e);
      try {
        await tasksCreate(newName);
        await tasksDelete(oldName);
      } catch (fallbackErr) {
        console.warn('Fallback rename failed:', fallbackErr);
      }
      return tasksGetSummary();
    }
  }
  const raw = await invoke<unknown[]>('tasks_rename', { oldName, newName });
  return (Array.isArray(raw) ? raw : []).map(normalizeTaskStatsSummary);
};

export const tasksRestore = async (name: string): Promise<TaskStatsSummary[]> => {
  if (isRemoteMode()) {
    try {
      const res = await remoteFetch<unknown[]>('/api/tasks/restore', {
        method: 'POST',
        body: JSON.stringify({ name }),
      });
      return (Array.isArray(res) ? res : []).map(normalizeTaskStatsSummary);
    } catch (e) {
      console.warn('Remote /api/tasks/restore failed or not supported yet, falling back to create:', e);
      try {
        await tasksCreate(name);
      } catch (fallbackErr) {
        console.warn('Fallback restore failed:', fallbackErr);
      }
      return tasksGetSummary();
    }
  }
  const raw = await invoke<unknown[]>('tasks_restore', { name });
  return (Array.isArray(raw) ? raw : []).map(normalizeTaskStatsSummary);
};

export const tasksToggleComplete = async (name: string, completed: boolean): Promise<TaskItem[]> => {
  if (isRemoteMode()) {
    try {
      const raw = await remoteFetch<unknown[]>('/api/tasks/complete', {
        method: 'POST',
        body: JSON.stringify({ name, completed }),
      });
      return (Array.isArray(raw) ? raw : []).map(normalizeTask);
    } catch (e) {
      console.warn('Remote server may not support /api/tasks/complete yet:', e);
      if (isTauri) {
        const raw = await invoke<unknown[]>('tasks_toggle_complete', { name, completed });
        return (Array.isArray(raw) ? raw : []).map(normalizeTask);
      }
      return tasksList();
    }
  }
  const raw = await invoke<unknown[]>('tasks_toggle_complete', { name, completed });
  return (Array.isArray(raw) ? raw : []).map(normalizeTask);
};

export const tasksDelete = async (name: string): Promise<TaskItem[]> => {
  if (isRemoteMode()) {
    try {
      const raw = await remoteFetch<unknown[]>('/api/tasks/delete', {
        method: 'POST',
        body: JSON.stringify({ name }),
      });
      return (Array.isArray(raw) ? raw : []).map(normalizeTask);
    } catch (e) {
      console.warn('Remote server may not support /api/tasks/delete yet:', e);
      if (isTauri) {
        const raw = await invoke<unknown[]>('tasks_delete', { name });
        return (Array.isArray(raw) ? raw : []).map(normalizeTask);
      }
      return tasksList();
    }
  }
  const raw = await invoke<unknown[]>('tasks_delete', { name });
  return (Array.isArray(raw) ? raw : []).map(normalizeTask);
};

// --- Preset commands ---

export function normalizePreset(item: unknown): PresetItem {
  if (!item || typeof item !== 'object') {
    return {
      id: 1,
      name: 'Default',
      work_secs: 1500,
      short_break_secs: 300,
      long_break_secs: 900,
      rounds: 4,
    };
  }
  const obj = item as Record<string, unknown>;
  return {
    id: Number(obj.id ?? 1),
    name: String(obj.name ?? 'Default'),
    work_secs: Number(obj.work_secs ?? obj.workSecs ?? 1500),
    short_break_secs: Number(obj.short_break_secs ?? obj.shortBreakSecs ?? 300),
    long_break_secs: Number(obj.long_break_secs ?? obj.longBreakSecs ?? 900),
    rounds: Number(obj.rounds ?? 4),
  };
}

export const presetsList = async (): Promise<PresetItem[]> => {
  let raw: unknown[];
  if (isRemoteMode()) {
    try {
      raw = await remoteFetch<unknown[]>('/api/presets');
    } catch (e) {
      console.warn('Failed to fetch presets from remote:', e);
      if (isTauri) {
        raw = await invoke<unknown[]>('presets_list');
      } else {
        throw e;
      }
    }
  } else {
    raw = await invoke<unknown[]>('presets_list');
  }
  const list = (Array.isArray(raw) ? raw : []).map(normalizePreset);
  if (list.length === 0) {
    return [{ id: 1, name: 'Default', work_secs: 1500, short_break_secs: 300, long_break_secs: 900, rounds: 4 }];
  }
  return list;
};

export const presetsCreate = async (
  name: string,
  work_secs: number,
  short_break_secs: number,
  long_break_secs: number,
  rounds: number
): Promise<PresetItem[]> => {
  if (isRemoteMode()) {
    try {
      const raw = await remoteFetch<unknown[]>('/api/presets', {
        method: 'POST',
        body: JSON.stringify({ name, work_secs, short_break_secs, long_break_secs, rounds }),
      });
      return (Array.isArray(raw) ? raw : []).map(normalizePreset);
    } catch (e) {
      console.warn('Remote server may not support /api/presets create yet:', e);
      if (isTauri) {
        const raw = await invoke<unknown[]>('presets_create', {
          name,
          workSecs: work_secs,
          shortBreakSecs: short_break_secs,
          longBreakSecs: long_break_secs,
          rounds,
        });
        return (Array.isArray(raw) ? raw : []).map(normalizePreset);
      }
      return presetsList();
    }
  }
  const raw = await invoke<unknown[]>('presets_create', {
    name,
    workSecs: work_secs,
    shortBreakSecs: short_break_secs,
    longBreakSecs: long_break_secs,
    rounds,
  });
  return (Array.isArray(raw) ? raw : []).map(normalizePreset);
};

export const presetsUpdate = async (
  id: number,
  name: string,
  work_secs: number,
  short_break_secs: number,
  long_break_secs: number,
  rounds: number
): Promise<PresetItem[]> => {
  if (isRemoteMode()) {
    try {
      const raw = await remoteFetch<unknown[]>('/api/presets/update', {
        method: 'POST',
        body: JSON.stringify({ id, name, work_secs, short_break_secs, long_break_secs, rounds }),
      });
      return (Array.isArray(raw) ? raw : []).map(normalizePreset);
    } catch (e) {
      console.warn('Remote server may not support /api/presets/update yet:', e);
      if (isTauri) {
        const raw = await invoke<unknown[]>('presets_update', {
          id,
          name,
          workSecs: work_secs,
          shortBreakSecs: short_break_secs,
          longBreakSecs: long_break_secs,
          rounds,
        });
        return (Array.isArray(raw) ? raw : []).map(normalizePreset);
      }
      return presetsList();
    }
  }
  const raw = await invoke<unknown[]>('presets_update', {
    id,
    name,
    workSecs: work_secs,
    shortBreakSecs: short_break_secs,
    longBreakSecs: long_break_secs,
    rounds,
  });
  return (Array.isArray(raw) ? raw : []).map(normalizePreset);
};

export const presetsDelete = async (id: number): Promise<PresetItem[]> => {
  if (isRemoteMode()) {
    try {
      const raw = await remoteFetch<unknown[]>('/api/presets/delete', {
        method: 'POST',
        body: JSON.stringify({ id }),
      });
      return (Array.isArray(raw) ? raw : []).map(normalizePreset);
    } catch (e) {
      console.warn('Remote server may not support /api/presets/delete yet:', e);
      if (isTauri) {
        const raw = await invoke<unknown[]>('presets_delete', { id });
        return (Array.isArray(raw) ? raw : []).map(normalizePreset);
      }
      return presetsList();
    }
  }
  const raw = await invoke<unknown[]>('presets_delete', { id });
  return (Array.isArray(raw) ? raw : []).map(normalizePreset);
};

export const presetsSelect = async (name: string): Promise<Settings> => {
  if (isRemoteMode()) {
    try {
      return await remoteFetch<Settings>('/api/presets/select', {
        method: 'POST',
        body: JSON.stringify({ name }),
      });
    } catch (e) {
      console.warn('Remote server may not support /api/presets/select yet:', e);
      if (isTauri) {
        return invoke<Settings>('presets_select', { name });
      }
      throw e;
    }
  }
  return invoke<Settings>('presets_select', { name });
};

// --- Obsidian commands ---

export const obsidianExportWeekly = async (weekOffset = 0): Promise<ObsidianExportResult> => {
  if (isRemoteMode()) {
    try {
      const res = await remoteFetch<ObsidianExportResult>('/api/export/obsidian', {
        method: 'POST',
        body: JSON.stringify({ week_offset: weekOffset }),
      });
      if (res.exported && res.content && res.filename && isTauri) {
        const savedPath = await invoke<string>('obsidian_save_file', {
          filename: res.filename,
          content: res.content,
        });
        return {
          ...res,
          file_path: savedPath,
        };
      }
      return res;
    } catch (e) {
      console.warn('Remote obsidian export failed, falling back to local:', e);
      if (isTauri) {
        return invoke<ObsidianExportResult>('obsidian_export_weekly', { weekOffset });
      }
      throw e;
    }
  }
  return invoke<ObsidianExportResult>('obsidian_export_weekly', { weekOffset });
};

// --- Settings commands ---

export const getSettings = async () => {
  if (isRemoteMode()) {
    try {
      return await remoteFetch<Settings>('/api/settings');
    } catch (err) {
      console.warn('Failed to fetch settings from remote, falling back to local:', err);
      if (isTauri) {
        return invoke<Settings>('settings_get');
      }
      throw err;
    }
  }
  return invoke<Settings>('settings_get');
};

/** Save a single setting key/value pair and receive the full updated settings. */
export const setSetting = async (key: string, value: string) => {
  if (isRemoteMode()) {
    return remoteFetch<Settings>('/api/settings', {
      method: 'POST',
      body: JSON.stringify({ key, value }),
    });
  }
  return invoke<Settings>('settings_set', { key, value });
};

export const resetSettings = async () => {
  if (isRemoteMode()) {
    return remoteFetch<Settings>('/api/settings/reset', { method: 'POST' });
  }
  return invoke<Settings>('settings_reset_defaults');
};

export const reloadShortcuts = async () => {
  if (isRemoteMode()) return;
  return invoke<void>('shortcuts_reload');
};

// --- Theme commands ---

export const getThemes = async () => {
  if (isRemoteMode()) {
    try {
      return await remoteFetch<Theme[]>('/api/themes');
    } catch (err) {
      console.warn('Failed to fetch themes from remote, falling back to local:', err);
      if (isTauri) {
        return invoke<Theme[]>('themes_list');
      }
      throw err;
    }
  }
  return invoke<Theme[]>('themes_list');
};

// --- Notification commands ---

export const notificationShow = async (title: string, body: string) => {
  if (isTauri) {
    try {
      return await invoke<void>('notification_show', { title, body });
    } catch (err) {
      console.warn('Tauri notification_show failed, attempting browser fallback:', err);
    }
  }
  if (typeof window !== 'undefined' && 'Notification' in window) {
    if (Notification.permission === 'granted') {
      new Notification(title, { body, icon: '/app-icon.png' });
    } else if (Notification.permission !== 'denied') {
      Notification.requestPermission().then((permission) => {
        if (permission === 'granted') {
          new Notification(title, { body, icon: '/app-icon.png' });
        }
      });
    }
  }
};

// --- Window commands ---

export const setWindowVisibility = async (visible: boolean) => {
  if (!isTauri) return;
  return invoke<void>('window_set_visibility', { visible });
};

// --- Audio commands ---

export const getCustomAudioInfo = async () => {
  if (isRemoteMode() || !isTauri) {
    return { work_alert: null, short_break_alert: null, long_break_alert: null };
  }
  return invoke<CustomAudioInfo>('audio_get_custom_info');
};

/** Copy `srcPath` to the config dir for `cue`; returns the display name. */
export const setCustomAudio = async (cue: string, srcPath: string) => {
  if (!isTauri) return '';
  return invoke<string>('audio_set_custom', { cue, srcPath });
};

/** Delete the custom file for `cue` and revert to the built-in sound. */
export const clearCustomAudio = async (cue: string) => {
  if (!isTauri) return;
  return invoke<void>('audio_clear_custom', { cue });
};

/** Open a native file picker filtered to audio formats. Returns a path or null. */
export const openAudioFilePicker = (): Promise<string | null> => {
  if (!isTauri) return Promise.resolve(null);
  return dialogOpen({
    multiple: false,
    filters: [{ name: 'Audio', extensions: ['mp3', 'wav', 'ogg'] }],
  }) as Promise<string | null>;
};

// --- Diagnostic log commands ---

/** Open the application log directory in the OS file manager. */
export const openLogDir = async () => {
  if (!isTauri) return;
  return invoke<void>('open_log_dir');
};

/** Return the resolved log directory path as a string. */
export const getLogDir = async () => {
  if (!isTauri) return '';
  return invoke<string>('get_log_dir');
};

/** Return the compile-time build version string (e.g. `1.0.0-dev.80+20b2d87`). */
export const appVersion = async () => {
  if (!isTauri) return '1.7.1 (Web/Docker)';
  return invoke<string>('app_version');
};

// --- Sessions commands ---

export const clearSessionHistory = async () => {
  if (isRemoteMode()) {
    await remoteFetch<void>('/api/sessions/clear', { method: 'POST' });
    return;
  }
  return invoke<void>('sessions_clear');
};

// --- Stats commands ---

/** Daily + weekly data + streak in one call (Today and This Week tabs). */
export const statsGetDetailed = async () => {
  if (isRemoteMode()) {
    return remoteFetch<DetailedStats>('/api/stats/detailed');
  }
  return invoke<DetailedStats>('stats_get_detailed');
};

/** Daily stats for a specific date (YYYY-MM-DD). */
export const statsGetDailyByDate = async (date: string): Promise<DailyStats> => {
  if (isRemoteMode()) {
    return remoteFetch<DailyStats>(`/api/stats/daily?date=${encodeURIComponent(date)}`);
  }
  return invoke<DailyStats>('stats_get_daily_by_date', { date });
};

/** Calendar week stats (Monday to Sunday) and task breakdown for a week offset. */
export const statsGetWeeklyByOffset = async (weekOffset: number): Promise<CalendarWeekStats> => {
  if (isRemoteMode()) {
    return remoteFetch<CalendarWeekStats>(`/api/stats/weekly?offset=${weekOffset}`);
  }
  return invoke<CalendarWeekStats>('stats_get_weekly_by_offset', { weekOffset });
};

/** Heatmap entries + lifetime totals (All Time tab). */
export const statsGetHeatmap = async () => {
  if (isRemoteMode()) {
    return remoteFetch<HeatmapStats>('/api/stats/heatmap');
  }
  return invoke<HeatmapStats>('stats_get_heatmap');
};

// --- Platform commands ---

export const accessibilityTrusted = async () => {
  if (!isTauri) return false;
  return invoke<boolean>('accessibility_trusted');
};

/** Returns true if the system tray is usable on this platform/install.
 *  On Linux this probes for libayatana-appindicator3 / libappindicator3 at
 *  runtime; on macOS and Windows it always returns true. */
export const traySupported = async () => {
  if (!isTauri) return false;
  return invoke<boolean>('tray_supported');
};

// --- Updater commands ---

/** Check for an available update. Returns update info or null if already up to date. */
export const checkUpdate = async () => {
  if (!isTauri) return null;
  return invoke<UpdateInfo | null>('check_update');
};

/** Download, install, and immediately relaunch with the pending update. */
export const installUpdate = async () => {
  if (!isTauri) return;
  return invoke<void>('install_update');
};

// --- Event listeners ---

export const onTimerTick = (
  cb: (payload: { elapsed_secs: number; total_secs: number }) => void
): Promise<UnlistenFn> => {
  if (isRemoteMode()) {
    return Promise.resolve(addWsListener('timer:tick', cb));
  }
  return listen<{ elapsed_secs: number; total_secs: number }>('timer:tick', (e) => cb(e.payload));
};

export const onTimerPaused = (
  cb: (payload: { elapsed_secs: number }) => void
): Promise<UnlistenFn> => {
  if (isRemoteMode()) {
    return Promise.resolve(addWsListener('timer:paused', cb));
  }
  return listen<{ elapsed_secs: number }>('timer:paused', (e) => cb(e.payload));
};

export const onTimerResumed = (
  cb: (payload: { elapsed_secs: number }) => void
): Promise<UnlistenFn> => {
  if (isRemoteMode()) {
    return Promise.resolve(addWsListener('timer:resumed', cb));
  }
  return listen<{ elapsed_secs: number }>('timer:resumed', (e) => cb(e.payload));
};

export const onRoundChange = (cb: (state: TimerState) => void): Promise<UnlistenFn> => {
  if (isRemoteMode()) {
    return Promise.resolve(addWsListener('timer:round-change', cb));
  }
  return listen<TimerState>('timer:round-change', (e) => cb(e.payload));
};

export const onTimerReset = (cb: (state: TimerState) => void): Promise<UnlistenFn> => {
  if (isRemoteMode()) {
    return Promise.resolve(addWsListener('timer:reset', cb));
  }
  return listen<TimerState>('timer:reset', (e) => cb(e.payload));
};

export const onSettingsChanged = (cb: (settings: Settings) => void): Promise<UnlistenFn> => {
  if (isRemoteMode()) {
    return Promise.resolve(addWsListener('settings:changed', cb));
  }
  return listen<Settings>('settings:changed', (e) => cb(e.payload));
};

export const onThemesChanged = (cb: (themes: Theme[]) => void): Promise<UnlistenFn> => {
  if (isRemoteMode()) {
    return Promise.resolve(addWsListener('themes:changed', cb));
  }
  return listen<Theme[]>('themes:changed', (e) => cb(e.payload));
};

export const onSessionsCleared = (cb: () => void): Promise<UnlistenFn> => {
  if (isRemoteMode()) {
    return Promise.resolve(addWsListener('sessions:cleared', cb));
  }
  return listen<void>('sessions:cleared', () => cb());
};

export const onPresetsChanged = (cb: (presets: PresetItem[]) => void): Promise<UnlistenFn> => {
  if (isRemoteMode()) {
    return Promise.resolve(addWsListener('presets:changed', cb));
  }
  return listen<PresetItem[]>('presets:changed', (e) => cb(e.payload));
};
