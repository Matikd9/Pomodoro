import { writable, derived, get } from 'svelte/store';
import {
  presetsList,
  presetsCreate,
  presetsUpdate,
  presetsDelete,
  presetsSelect,
  timerReset,
  getTimerState,
} from '$lib/ipc';
import { settings } from '$lib/stores/settings';
import { timerState } from '$lib/stores/timer';
import type { PresetItem } from '$lib/types';

export const defaultPreset: PresetItem = {
  id: 1,
  name: 'Default',
  work_secs: 1500,
  short_break_secs: 300,
  long_break_secs: 900,
  rounds: 4,
};

export const presets = writable<PresetItem[]>([defaultPreset]);

// The active preset name comes directly from $settings.active_preset (single source of truth in SQLite).
export const activePresetName = derived(settings, ($s) => $s.active_preset || 'Default');

// The active preset object resolves from $presets and $settings.active_preset.
export const activePreset = derived(
  [presets, settings],
  ([$presets, $settings]) => {
    const name = $settings.active_preset || 'Default';
    return $presets.find((p) => p.name === name) || $presets[0] || defaultPreset;
  }
);

export async function loadPresets(): Promise<void> {
  try {
    const list = await presetsList();
    if (list && list.length > 0) {
      presets.set(list);
    }
  } catch (e) {
    console.error('Failed to load presets:', e);
  }
}

export async function selectPreset(preset: PresetItem): Promise<void> {
  try {
    const updatedSettings = await presetsSelect(preset.name);
    settings.set(updatedSettings);

    await timerReset();
    const updated = await getTimerState();
    timerState.set(updated);
  } catch (err) {
    console.error('Failed to apply preset settings:', err);
  }
}

export async function createPreset(name: string): Promise<PresetItem | null> {
  const clean = name.trim();
  if (!clean) return null;

  const current = get(settings);
  const workSecs = current.time_work_secs || 1500;
  const shortSecs = current.time_short_break_secs || 300;
  const longSecs = current.time_long_break_secs || 900;
  const rounds = current.long_break_interval || 4;

  try {
    const updated = await presetsCreate(clean, workSecs, shortSecs, longSecs, rounds);
    presets.set(updated);
    const created = updated.find((p) => p.name === clean) || updated[updated.length - 1];
    if (created) {
      await selectPreset(created);
      return created;
    }
  } catch (e) {
    console.error('Failed to create preset:', e);
  }
  return null;
}

export async function deletePreset(preset: PresetItem): Promise<void> {
  if (preset.name === 'Default') return;

  try {
    const updated = await presetsDelete(preset.id);
    presets.set(updated);

    const currentActive = get(activePresetName);
    if (currentActive === preset.name) {
      const fallback = updated[0] || defaultPreset;
      await selectPreset(fallback);
    }
  } catch (e) {
    console.error('Failed to delete preset:', e);
  }
}

/**
 * Synchronizes any changes made to timer durations in Settings > Timer with the currently active preset.
 */
export async function syncActivePresetWithSettings(patch: {
  work_secs?: number;
  short_break_secs?: number;
  long_break_secs?: number;
  rounds?: number;
}): Promise<void> {
  const active = get(activePreset);
  if (!active) return;

  const workSecs = patch.work_secs ?? active.work_secs;
  const shortSecs = patch.short_break_secs ?? active.short_break_secs;
  const longSecs = patch.long_break_secs ?? active.long_break_secs;
  const rounds = patch.rounds ?? active.rounds;

  try {
    const updated = await presetsUpdate(
      active.id,
      active.name,
      workSecs,
      shortSecs,
      longSecs,
      rounds
    );
    presets.set(updated);
  } catch (e) {
    console.error('Failed to sync active preset with settings:', e);
  }
}
