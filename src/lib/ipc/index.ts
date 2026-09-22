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
  HeatmapStats,
  UpdateInfo,
} from '$lib/types';

// --- Remote Server Configuration ---

const REMOTE_URL_KEY = 'pomotroid_remote_url';

export function getRemoteServerUrl(): string | null {
  if (typeof window === 'undefined') return null;
  const configured = localStorage.getItem(REMOTE_URL_KEY)?.trim();
  if (configured) {
    return configured.replace(/\/+$/, '');
  }
  // When running in a standard web browser (e.g. mobile phone), use current origin
  if (!isTauri) {
    return window.location.origin;
  }
  return null;
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

export const tasksList = async () => {
  if (isRemoteMode()) {
    return remoteFetch<string[]>('/api/tasks');
  }
  return invoke<string[]>('tasks_list');
};

export const tasksCreate = async (name: string) => {
  if (isRemoteMode()) {
    return remoteFetch<string[]>('/api/tasks', {
      method: 'POST',
      body: JSON.stringify({ name }),
    });
  }
  return invoke<string[]>('tasks_create', { name });
};

// --- Settings commands ---

export const getSettings = async () => {
  if (isRemoteMode()) {
    return remoteFetch<Settings>('/api/settings');
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
    return remoteFetch<Theme[]>('/api/themes');
  }
  return invoke<Theme[]>('themes_list');
};

// --- Notification commands ---

export const notificationShow = async (title: string, body: string) => {
  if (isTauri && !isRemoteMode()) {
    return invoke<void>('notification_show', { title, body });
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
