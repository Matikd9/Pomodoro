<script lang="ts">
  import { onMount } from 'svelte';
  import {
    presets,
    activePreset,
    activePresetName,
    loadPresets,
    selectPreset,
    createPreset,
    deletePreset,
  } from '$lib/stores/presets';
  import type { PresetItem } from '$lib/types';
  import * as m from '$paraglide/messages.js';

  let isOpen = $state(false);
  let newPresetInput = $state('');
  let popoverEl = $state<HTMLElement | null>(null);
  let inputEl = $state<HTMLInputElement | null>(null);

  onMount(() => {
    loadPresets();

    function handleClickOutside(e: MouseEvent) {
      if (isOpen && popoverEl && !popoverEl.contains(e.target as Node)) {
        isOpen = false;
      }
    }

    function handleKeyDown(e: KeyboardEvent) {
      if (isOpen && e.key === 'Escape') {
        isOpen = false;
      }
    }

    window.addEventListener('click', handleClickOutside);
    window.addEventListener('keydown', handleKeyDown);

    return () => {
      window.removeEventListener('click', handleClickOutside);
      window.removeEventListener('keydown', handleKeyDown);
    };
  });

  $effect(() => {
    if (isOpen) {
      setTimeout(() => inputEl?.focus(), 50);
    }
  });

  function toggleOpen(e: MouseEvent) {
    e.stopPropagation();
    isOpen = !isOpen;
    if (isOpen) {
      loadPresets();
    }
  }

  async function handleSelect(preset: PresetItem) {
    await selectPreset(preset);
    isOpen = false;
  }

  async function handleCreatePreset() {
    const trimmed = newPresetInput.trim();
    if (!trimmed) return;
    newPresetInput = '';
    await createPreset(trimmed);
  }

  function handleInputKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      handleCreatePreset();
    }
  }

  async function handleDelete(preset: PresetItem, e: MouseEvent) {
    e.stopPropagation();
    await deletePreset(preset);
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
    <span class="preset-name">{$activePreset?.name || 'Default'}</span>
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
      <!-- Create Input Row (Matches TaskSelector style) -->
      <div class="input-row">
        <input
          bind:this={inputEl}
          bind:value={newPresetInput}
          onkeydown={handleInputKeydown}
          type="text"
          placeholder={m.preset_create_placeholder()}
          maxlength="30"
          class="preset-input"
        />
        <button
          class="btn-create"
          onclick={handleCreatePreset}
          disabled={!newPresetInput.trim()}
          title={m.preset_create_button()}
          aria-label={m.preset_create_button()}
        >
          <svg
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2.5"
            stroke-linecap="round"
          >
            <line x1="12" y1="5" x2="12" y2="19" />
            <line x1="5" y1="12" x2="19" y2="12" />
          </svg>
        </button>
      </div>

      <!-- Presets List -->
      <div class="presets-list">
        {#each $presets as preset (preset.id || preset.name)}
          <div class="preset-row" class:selected={preset.name === $activePresetName}>
            <button
              type="button"
              class="preset-item-btn"
              onclick={() => handleSelect(preset)}
              title={preset.name}
            >
              <div class="preset-item-info">
                <div class="preset-item-top">
                  <span class="preset-item-name">{preset.name}</span>
                  {#if preset.name === $activePresetName}
                    <span class="active-badge">{m.preset_active_badge()}</span>
                  {/if}
                </div>
                <div class="preset-item-summary">
                  {Math.round(preset.work_secs / 60)}m / {Math.round(preset.short_break_secs / 60)}m / {Math.round(preset.long_break_secs / 60)}m • {preset.rounds} rds
                </div>
              </div>
            </button>

            <!-- Delete action button (Default is protected) -->
            {#if preset.name !== 'Default'}
              <div class="preset-actions">
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
              </div>
            {/if}
          </div>
        {/each}
      </div>
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
    max-width: 145px;
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

  /* Popover aligns to the left edge of badge, constrained nicely within the window */
  .preset-popover {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    width: 220px;
    max-width: calc(100vw - 32px);
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
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .input-row {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-bottom: 2px;
  }

  .preset-input {
    flex: 1;
    background: var(--color-background);
    border: 1px solid color-mix(in oklch, var(--color-foreground) 15%, transparent);
    border-radius: 4px;
    padding: 4px 7px;
    font-size: 0.72rem;
    color: var(--color-foreground);
    outline: none;
    transition: border-color var(--transition-snappy);
    min-width: 0;
  }

  .preset-input:focus {
    border-color: var(--color-focus-round);
  }

  .btn-create {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    background: var(--color-focus-round);
    border: none;
    border-radius: 4px;
    cursor: pointer;
    color: #fff;
    flex-shrink: 0;
    transition: all var(--transition-snappy);
  }

  .btn-create:hover:not(:disabled) {
    opacity: 0.9;
    transform: scale(1.04);
  }

  .btn-create:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .presets-list {
    display: flex;
    flex-direction: column;
    max-height: 190px;
    overflow-y: auto;
    gap: 3px;
  }

  .preset-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 4px 6px;
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
    font-size: 0.58rem;
    text-transform: uppercase;
    font-weight: 700;
    padding: 1px 4px;
    border-radius: 3px;
    background: color-mix(in oklch, var(--color-focus-round) 20%, transparent);
    color: var(--color-focus-round);
    letter-spacing: 0.03em;
  }

  .preset-item-summary {
    font-size: 0.63rem;
    color: var(--color-foreground-darker);
    opacity: 0.8;
  }

  .preset-actions {
    display: flex;
    align-items: center;
    margin-left: 4px;
    flex-shrink: 0;
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

  .delete-btn:hover {
    color: #e05252;
    background: color-mix(in oklch, #e05252 15%, transparent);
  }
</style>
