<script lang="ts">
  import { timerState } from '$lib/stores/timer';
  import { timerToggle, timerSkip } from '$lib/ipc';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { isTauri } from '$lib/utils/platform';
  import Tooltip from './Tooltip.svelte';
  import type { TimerMode } from '$lib/types';
  import * as m from '$paraglide/messages.js';

  interface Props {
    mode?: TimerMode;
    onExpand: () => void;
    onClose?: () => void;
  }

  let { mode = 'pomodoro', onExpand, onClose }: Props = $props();

  let snap = $derived($timerState);

  // In continuous mode: count up (stopwatch).
  // In pomodoro mode: count down (timer).
  const isContinuous = $derived(mode === 'continuous' || snap.total_secs === 0);

  // Remaining time in pomodoro mode, elapsed in continuous mode
  const displaySecs = $derived.by(() => {
    if (isContinuous) {
      return snap.elapsed_secs;
    }
    const total = snap.total_secs > 0 ? snap.total_secs : 1500;
    return Math.max(0, total - snap.elapsed_secs);
  });

  const formattedTime = $derived.by(() => {
    const s = displaySecs;
    const hours = Math.floor(s / 3600);
    const mins = Math.floor((s % 3600) / 60);
    const secs = s % 60;
    const pad = (n: number) => String(n).padStart(2, '0');
    if (hours > 0 && isContinuous) {
      return `${hours}:${pad(mins)}:${pad(secs)}`;
    }
    return `${pad(mins)}:${pad(secs)}`;
  });

  // Break state in Pomodoro mode
  const isBreak = $derived(
    !isContinuous &&
    (snap.round_type === 'short-break' || snap.round_type === 'long-break')
  );

  // Color: Red for focus/continuous, Green for break
  const timerColor = $derived(
    isBreak ? 'var(--color-short-round)' : 'var(--color-focus-round)'
  );

  const roundBadgeText = $derived.by(() => {
    if (isContinuous) return 'Continuo';
    if (isBreak) return 'Descanso';
    return 'Focus';
  });

  async function closeApp() {
    if (onClose) {
      onClose();
      return;
    }
    if (isTauri) {
      await getCurrentWebviewWindow().close();
    }
  }
</script>

<div class="mini-player" data-tauri-drag-region>
  <!-- Left: Time + Status Dot + Round Badge -->
  <Tooltip text={`${roundBadgeText}: ${formattedTime}`}>
    <div class="time-block" data-tauri-drag-region>
      <span
        class="status-dot"
        class:running={snap.is_running}
        style="background-color: {timerColor}; color: {timerColor};"
      ></span>
      <div class="time-col" data-tauri-drag-region>
        <span class="time-text" style="color: {timerColor}">
          {formattedTime}
        </span>
        <span class="round-badge" style="color: {timerColor}">
          {roundBadgeText}
        </span>
      </div>
    </div>
  </Tooltip>

  <!-- Right: Action Controls -->
  <div class="controls-block">
    <!-- Play / Pause -->
    <Tooltip text={snap.is_running ? 'Pause' : 'Play'}>
      <button class="mini-btn main-action" onclick={timerToggle} aria-label={snap.is_running ? 'Pause' : 'Play'}>
        {#if snap.is_running}
          <svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor">
            <rect x="6" y="4" width="4" height="16" rx="1.5" />
            <rect x="14" y="4" width="4" height="16" rx="1.5" />
          </svg>
        {:else}
          <svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor">
            <polygon points="7,4 19,12 7,20" />
          </svg>
        {/if}
      </button>
    </Tooltip>

    <!-- Skip (only in pomodoro mode) -->
    {#if !isContinuous}
      <Tooltip text={m.tooltip_skip()}>
        <button class="mini-btn" onclick={timerSkip} aria-label="Skip round">
          <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
            <polygon points="5,4 15,12 5,20" />
            <rect x="17" y="4" width="3" height="16" rx="1" />
          </svg>
        </button>
      </Tooltip>
    {/if}

    <!-- Expand to Normal Window -->
    <Tooltip text="Expand window">
      <button class="mini-btn" onclick={onExpand} aria-label="Expand Window">
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="10 2 14 2 14 6" />
          <line x1="14" y1="2" x2="8" y2="8" />
          <polyline points="6 14 2 14 2 10" />
          <line x1="2" y1="14" x2="8" y2="8" />
        </svg>
      </button>
    </Tooltip>

    <!-- Close -->
    <Tooltip text="Close">
      <button class="mini-btn close-btn" onclick={closeApp} aria-label="Close">
        <svg width="11" height="11" viewBox="0 0 12 12" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
          <line x1="2" y1="2" x2="10" y2="10" />
          <line x1="10" y1="2" x2="2" y2="10" />
        </svg>
      </button>
    </Tooltip>
  </div>
</div>

<style>
  .mini-player {
    width: 100vw;
    height: 100vh;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    background-color: var(--color-background);
    color: var(--color-foreground);
    user-select: none;
    overflow: hidden;
  }

  .time-block {
    display: flex;
    align-items: center;
    gap: 9px;
    cursor: grab;
  }

  .time-col {
    display: flex;
    flex-direction: column;
    justify-content: center;
    cursor: grab;
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    transition: background-color 0.2s ease, box-shadow 0.2s ease;
  }

  .status-dot.running {
    box-shadow: 0 0 6px currentColor;
  }

  .time-text {
    font-family: inherit;
    font-size: 21px;
    font-weight: 700;
    letter-spacing: -0.5px;
    line-height: 1;
  }

  .round-badge {
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    opacity: 0.85;
    margin-top: 2px;
    line-height: 1;
  }

  .controls-block {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .mini-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--color-foreground);
    opacity: 0.75;
    border-radius: 6px;
    cursor: pointer;
    transition: opacity 0.15s, background-color 0.15s, color 0.15s;
  }

  .mini-btn:hover {
    opacity: 1;
    background-color: rgba(255, 255, 255, 0.08);
  }

  .mini-btn.main-action {
    opacity: 1;
    color: var(--color-foreground);
    background-color: rgba(255, 255, 255, 0.1);
  }

  .mini-btn.main-action:hover {
    background-color: rgba(255, 255, 255, 0.18);
  }

  .mini-btn.close-btn:hover {
    color: #ff5f56;
    background-color: rgba(255, 95, 86, 0.15);
  }
</style>
