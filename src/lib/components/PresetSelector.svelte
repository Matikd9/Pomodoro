<script lang="ts">
  import { onMount } from 'svelte';
  import { settings } from '$lib/stores/settings';
  import { timerState } from '$lib/stores/timer';
  import {
    presetsList,
    presetsCreate,
    presetsUpdate,
    presetsDelete,
    setSetting,
    timerReset,
    getTimerState,
  } from '$lib/ipc';
  import type { PresetItem } from '$lib/types';
  import * as m from '$paraglide/messages.js';

  let isOpen = $state(false);
  let presets = $state<PresetItem[]>([
    { id: 1, name: 'Default', work_secs: 1500, short_break_secs: 300, long_break_secs: 900, rounds: 4 },
  ]);
  let activePresetName = $state('Default');
  let popoverEl = $state<HTMLElement | null>(null);

  // Form mode: null = list, 'create' = new preset, 'edit' = editing existing preset
  let formMode = $state<'create' | 'edit' | null>(null);
  let editId = $state<number | null>(null);
  let formName = $state('');
  let formWorkMins = $state(25);
  let formShortMins = $state(5);
  let formLongMins = $state(15);
  let formRounds = $state(4);
  let formError = $state<string | null>(null);

  const activePreset = $derived(
    presets.find((p) => p.name === activePresetName) || presets[0]
  );

  async function loadPresets() {
    try {
      const list = await presetsList();
      if (list && list.length > 0) {
        presets = list;
      }
    } catch (e) {
      console.error('Failed to load presets:', e);
    }
  }

  function resolveActivePreset() {
    if (typeof window !== 'undefined') {
      const saved = localStorage.getItem('pomotroid_active_preset');
      if (saved && presets.some((p) => p.name === saved)) {
        activePresetName = saved;
        return;
      }
    }

    // Try to match current settings
    const currentWork = $settings.time_work_secs;
    const currentShort = $settings.time_short_break_secs;
    const currentLong = $settings.time_long_break_secs;
    const currentRounds = $settings.long_break_interval;

    const matched = presets.find(
      (p) =>
        p.work_secs === currentWork &&
        p.short_break_secs === currentShort &&
        p.long_break_secs === currentLong &&
        p.rounds === currentRounds
    );

    if (matched) {
      activePresetName = matched.name;
    } else {
      activePresetName = presets[0]?.name || 'Default';
    }
  }

  onMount(() => {
    loadPresets().then(() => {
      resolveActivePreset();
    });

    function handleClickOutside(e: MouseEvent) {
      if (isOpen && popoverEl && !popoverEl.contains(e.target as Node)) {
        isOpen = false;
        formMode = null;
      }
    }

    function handleKeyDown(e: KeyboardEvent) {
      if (isOpen && e.key === 'Escape') {
        if (formMode !== null) {
          formMode = null;
        } else {
          isOpen = false;
        }
      }
    }

    window.addEventListener('click', handleClickOutside);
    window.addEventListener('keydown', handleKeyDown);

    return () => {
      window.removeEventListener('click', handleClickOutside);
      window.removeEventListener('keydown', handleKeyDown);
    };
  });

  async function selectPreset(preset: PresetItem) {
    activePresetName = preset.name;
    if (typeof window !== 'undefined') {
      localStorage.setItem('pomotroid_active_preset', preset.name);
    }

    try {
      await Promise.all([
        setSetting('time_work_secs', String(preset.work_secs)),
        setSetting('time_short_break_secs', String(preset.short_break_secs)),
        setSetting('time_long_break_secs', String(preset.long_break_secs)),
        setSetting('work_rounds', String(preset.rounds)),
      ]);

      settings.update((s) => ({
        ...s,
        time_work_secs: preset.work_secs,
        time_short_break_secs: preset.short_break_secs,
        time_long_break_secs: preset.long_break_secs,
        long_break_interval: preset.rounds,
      }));

      // Reset timer to apply new preset focus duration
      await timerReset();
      const updated = await getTimerState();
      timerState.set(updated);
    } catch (err) {
      console.error('Failed to apply preset settings:', err);
    }

    isOpen = false;
    formMode = null;
  }

  function startCreate() {
    formMode = 'create';
    editId = null;
    formName = '';
    formWorkMins = 25;
    formShortMins = 5;
    formLongMins = 15;
    formRounds = 4;
    formError = null;
  }

  function startEdit(preset: PresetItem, e?: MouseEvent) {
    e?.stopPropagation();
    formMode = 'edit';
    editId = preset.id;
    formName = preset.name;
    formWorkMins = Math.max(1, Math.round(preset.work_secs / 60));
    formShortMins = Math.max(1, Math.round(preset.short_break_secs / 60));
    formLongMins = Math.max(1, Math.round(preset.long_break_secs / 60));
    formRounds = Math.max(1, preset.rounds);
    formError = null;
  }

  function cancelForm() {
    formMode = null;
    editId = null;
    formError = null;
  }

  async function handleSaveForm() {
    const trimmed = formName.trim();
    if (!trimmed) {
      formError = 'Name is required';
      return;
    }

    const workSecs = Math.max(60, formWorkMins * 60);
    const shortSecs = Math.max(60, formShortMins * 60);
    const longSecs = Math.max(60, formLongMins * 60);
    const rounds = Math.max(1, Math.min(16, formRounds));

    try {
      let updated: PresetItem[];
      if (formMode === 'edit' && editId !== null) {
        updated = await presetsUpdate(editId, trimmed, workSecs, shortSecs, longSecs, rounds);
      } else {
        updated = await presetsCreate(trimmed, workSecs, shortSecs, longSecs, rounds);
      }
      presets = updated;

      // If we just edited the active preset, re-apply its durations
      if (activePresetName === trimmed || (formMode === 'edit' && activePreset?.id === editId)) {
        const found = updated.find((p) => p.name === trimmed || p.id === editId);
        if (found) {
          await selectPreset(found);
          return;
        }
      }

      formMode = null;
      editId = null;
      formError = null;
    } catch (e) {
      console.error('Failed to save preset:', e);
      formError = 'Failed to save preset';
    }
  }

  async function handleDelete(preset: PresetItem, e?: MouseEvent) {
    e?.stopPropagation();
    if (preset.name === 'Default') return;

    try {
      const updated = await presetsDelete(preset.id);
      presets = updated;
      if (activePresetName === preset.name) {
        const fallback = updated[0] || { id: 1, name: 'Default', work_secs: 1500, short_break_secs: 300, long_break_secs: 900, rounds: 4 };
        await selectPreset(fallback);
      }
    } catch (e) {
      console.error('Failed to delete preset:', e);
    }
  }

  function toggleOpen(e: MouseEvent) {
    e.stopPropagation();
    isOpen = !isOpen;
    if (isOpen) {
      loadPresets().then(resolveActivePreset);
      formMode = null;
    }
  }
</script>

<div class="preset-selector-container" bind:this={popoverEl}>
  <!-- Preset Pill / Badge Button -->
  <button
    class="preset-badge"
    class:active={isOpen}
    onclick={toggleOpen}
    title={m.preset_change_tooltip()}
    aria-label={m.preset_change_tooltip()}
  >
    <svg
      class="preset-icon"
      width="12"
      height="12"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <circle cx="12" cy="12" r="10" />
      <polyline points="12 6 12 12 16 14" />
    </svg>
    <span class="preset-name">{activePreset?.name || 'Default'}</span>
    <svg
      class="chevron"
      class:rotated={isOpen}
      width="10"
      height="10"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2.5"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <polyline points="6 9 12 15 18 9" />
    </svg>
  </button>

  <!-- Dropdown popover -->
  {#if isOpen}
    <div class="preset-popover" role="dialog" aria-modal="true">
      {#if formMode !== null}
        <!-- Form View (Create or Edit) -->
        <div class="form-container">
          <div class="form-title">
            {formMode === 'create' ? m.preset_new_button() : m.preset_edit_button()}
          </div>

          <div class="form-field">
            <label for="preset-name-input" class="field-label">{m.preset_name_label()}</label>
            <input
              id="preset-name-input"
              type="text"
              bind:value={formName}
              placeholder={m.preset_name_placeholder()}
              maxlength="32"
              class="form-input"
              disabled={formMode === 'edit' && formName === 'Default'}
            />
          </div>

          <div class="form-grid">
            <div class="form-field">
              <label for="preset-work-input" class="field-label">{m.preset_focus_mins_label()}</label>
              <input
                id="preset-work-input"
                type="number"
                min="1"
                max="180"
                bind:value={formWorkMins}
                class="form-input"
              />
            </div>
            <div class="form-field">
              <label for="preset-short-input" class="field-label">{m.preset_short_break_mins_label()}</label>
              <input
                id="preset-short-input"
                type="number"
                min="1"
                max="60"
                bind:value={formShortMins}
                class="form-input"
              />
            </div>
            <div class="form-field">
              <label for="preset-long-input" class="field-label">{m.preset_long_break_mins_label()}</label>
              <input
                id="preset-long-input"
                type="number"
                min="1"
                max="90"
                bind:value={formLongMins}
                class="form-input"
              />
            </div>
            <div class="form-field">
              <label for="preset-rounds-input" class="field-label">{m.preset_rounds_label()}</label>
              <input
                id="preset-rounds-input"
                type="number"
                min="1"
                max="16"
                bind:value={formRounds}
                class="form-input"
              />
            </div>
          </div>

          {#if formError}
            <div class="form-error">{formError}</div>
          {/if}

          <div class="form-actions">
            <button type="button" class="btn-cancel" onclick={cancelForm}>
              {m.preset_cancel_button()}
            </button>
            <button type="button" class="btn-save" onclick={handleSaveForm}>
              {m.preset_save_button()}
            </button>
          </div>
        </div>
      {:else}
        <!-- List View -->
        <div class="popover-header">
          <span class="header-title">{m.preset_label()}</span>
          <button type="button" class="btn-new-preset" onclick={startCreate} title={m.preset_new_button()}>
            <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
              <line x1="12" y1="5" x2="12" y2="19" />
              <line x1="5" y1="12" x2="19" y2="12" />
            </svg>
            <span>{m.preset_new_button()}</span>
          </button>
        </div>

        <div class="presets-list">
          {#each presets as preset (preset.id || preset.name)}
            <div class="preset-row" class:selected={preset.name === activePresetName}>
              <button
                type="button"
                class="preset-item-btn"
                onclick={() => selectPreset(preset)}
                title={preset.name}
              >
                <div class="preset-item-info">
                  <div class="preset-item-top">
                    <span class="preset-item-name">{preset.name}</span>
                    {#if preset.name === activePresetName}
                      <span class="active-badge">{m.preset_active_badge()}</span>
                    {/if}
                  </div>
                  <div class="preset-item-summary">
                    {Math.round(preset.work_secs / 60)}m / {Math.round(preset.short_break_secs / 60)}m / {Math.round(preset.long_break_secs / 60)}m • {preset.rounds} rds
                  </div>
                </div>
              </button>

              <div class="preset-actions">
                <button
                  type="button"
                  class="action-btn edit-btn"
                  onclick={(e) => startEdit(preset, e)}
                  title={m.preset_edit_button()}
                  aria-label={m.preset_edit_button()}
                >
                  <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.3">
                    <path d="M12 20h9" />
                    <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" />
                  </svg>
                </button>
                {#if preset.name !== 'Default'}
                  <button
                    type="button"
                    class="action-btn delete-btn"
                    onclick={(e) => handleDelete(preset, e)}
                    title={m.preset_delete_button()}
                    aria-label={m.preset_delete_button()}
                  >
                    <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.3">
                      <polyline points="3 6 5 6 21 6" />
                      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                    </svg>
                  </button>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .preset-selector-container {
    position: relative;
    display: inline-flex;
    justify-content: center;
    align-items: center;
    z-index: 35;
  }

  .preset-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 10px;
    border-radius: 9999px;
    background: color-mix(in oklch, var(--color-foreground) 8%, transparent);
    border: 1px solid color-mix(in oklch, var(--color-foreground) 12%, transparent);
    color: var(--color-foreground);
    font-size: 0.72rem;
    font-weight: 500;
    cursor: pointer;
    max-width: 150px;
    transition: all var(--transition-snappy);
    user-select: none;
  }

  .preset-badge:hover,
  .preset-badge.active {
    background: color-mix(in oklch, var(--color-foreground) 14%, transparent);
    border-color: color-mix(in oklch, var(--color-foreground) 24%, transparent);
  }

  .preset-icon {
    opacity: 0.65;
    flex-shrink: 0;
  }

  .preset-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chevron {
    opacity: 0.5;
    flex-shrink: 0;
    transition: transform 0.15s ease;
  }

  .chevron.rotated {
    transform: rotate(180deg);
  }

  .preset-popover {
    position: absolute;
    top: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
    width: 250px;
    background: var(--color-background-light);
    border: 1px solid color-mix(in oklch, var(--color-foreground) 16%, transparent);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    z-index: 60;
    animation: popover-in 0.15s ease-out;
  }

  @keyframes popover-in {
    from {
      opacity: 0;
      transform: translate(-50%, -4px);
    }
    to {
      opacity: 1;
      transform: translate(-50%, 0);
    }
  }

  .popover-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 2px 4px 6px 4px;
    border-bottom: 1px solid color-mix(in oklch, var(--color-foreground) 10%, transparent);
  }

  .header-title {
    font-size: 0.72rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--color-foreground-darker);
  }

  .btn-new-preset {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: transparent;
    border: none;
    color: var(--color-focus-round);
    font-size: 0.7rem;
    font-weight: 600;
    cursor: pointer;
    padding: 2px 4px;
    border-radius: 4px;
    transition: all var(--transition-snappy);
  }

  .btn-new-preset:hover {
    background: color-mix(in oklch, var(--color-focus-round) 15%, transparent);
  }

  .presets-list {
    display: flex;
    flex-direction: column;
    max-height: 180px;
    overflow-y: auto;
    gap: 3px;
  }

  .preset-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 3px 6px;
    border-radius: 5px;
    transition: background var(--transition-snappy);
  }

  .preset-row:hover {
    background: color-mix(in oklch, var(--color-foreground) 6%, transparent);
  }

  .preset-row.selected {
    background: color-mix(in oklch, var(--color-focus-round) 12%, transparent);
  }

  .preset-item-btn {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: transparent;
    border: none;
    color: var(--color-foreground);
    text-align: left;
    cursor: pointer;
    padding: 0;
    min-width: 0;
  }

  .preset-item-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .preset-item-top {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .preset-item-name {
    font-size: 0.74rem;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .preset-row.selected .preset-item-name {
    color: var(--color-focus-round);
  }

  .active-badge {
    font-size: 0.6rem;
    text-transform: uppercase;
    font-weight: 700;
    padding: 1px 4px;
    border-radius: 3px;
    background: color-mix(in oklch, var(--color-focus-round) 20%, transparent);
    color: var(--color-focus-round);
    letter-spacing: 0.03em;
  }

  .preset-item-summary {
    font-size: 0.64rem;
    color: var(--color-foreground-darker);
    opacity: 0.8;
  }

  .preset-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
    margin-left: 4px;
  }

  .action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    background: transparent;
    border: none;
    border-radius: 3px;
    cursor: pointer;
    color: var(--color-foreground-darker);
    opacity: 0.65;
    transition: all var(--transition-snappy);
  }

  .action-btn:hover {
    opacity: 1;
  }

  .edit-btn:hover {
    color: var(--color-focus-round);
    background: color-mix(in oklch, var(--color-focus-round) 15%, transparent);
  }

  .delete-btn:hover {
    color: #e05252;
    background: color-mix(in oklch, #e05252 15%, transparent);
  }

  /* Form Styles */
  .form-container {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 2px;
  }

  .form-title {
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--color-foreground);
    margin-bottom: 2px;
  }

  .form-field {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .field-label {
    font-size: 0.65rem;
    font-weight: 500;
    color: var(--color-foreground-darker);
  }

  .form-input {
    background: var(--color-background);
    border: 1px solid color-mix(in oklch, var(--color-foreground) 15%, transparent);
    border-radius: 4px;
    padding: 4px 6px;
    font-size: 0.72rem;
    color: var(--color-foreground);
    outline: none;
    transition: border-color var(--transition-snappy);
    width: 100%;
    box-sizing: border-box;
  }

  .form-input:focus {
    border-color: var(--color-focus-round);
  }

  .form-input:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .form-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }

  .form-error {
    font-size: 0.65rem;
    color: #e05252;
  }

  .form-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    margin-top: 4px;
  }

  .btn-cancel {
    background: transparent;
    border: 1px solid color-mix(in oklch, var(--color-foreground) 15%, transparent);
    border-radius: 4px;
    padding: 4px 8px;
    font-size: 0.7rem;
    color: var(--color-foreground);
    cursor: pointer;
  }

  .btn-cancel:hover {
    background: color-mix(in oklch, var(--color-foreground) 8%, transparent);
  }

  .btn-save {
    background: var(--color-focus-round);
    border: none;
    border-radius: 4px;
    padding: 4px 10px;
    font-size: 0.7rem;
    font-weight: 600;
    color: #fff;
    cursor: pointer;
  }

  .btn-save:hover {
    opacity: 0.9;
  }
</style>
