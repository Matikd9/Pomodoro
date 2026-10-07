<script lang="ts">
  // Orchestrator component. Subscribes to timer events, owns keyboard listener,
  // and renders TimerDial + TimerDisplay + TimerFooter.
  import { onMount } from 'svelte';
  import {
    timerToggle,
    timerRestartRound,
    timerSkip,
    timerReset,
    getTimerState,
    onTimerTick,
    onTimerPaused,
    onTimerResumed,
    onRoundChange,
    onTimerReset,
    setSetting,
  } from '$lib/ipc';
  import { timerState } from '$lib/stores/timer';
  import { settings } from '$lib/stores/settings';
  import { fade } from 'svelte/transition';
  import TimerDial from './TimerDial.svelte';
  import TimerDisplay from './TimerDisplay.svelte';
  import TimerFooter from './TimerFooter.svelte';
  import MiniControls from './MiniControls.svelte';
  import Tooltip from './Tooltip.svelte';
  import TaskSelector from './TaskSelector.svelte';
  import PresetSelector from './PresetSelector.svelte';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import type { TimerMode } from '$lib/types';
  import * as m from '$paraglide/messages.js';
  import { notificationShow } from '$lib/ipc';

  interface Props {
    isCompact?: boolean;
    uiScale?: number;
  }

  let { isCompact = false, uiScale = 1 }: Props = $props();

  // Active timer mode: 'pomodoro' (default) or 'continuous'
  let timerMode = $state<TimerMode>('pomodoro');
  // Base elapsed seconds accumulated across auto-skipped rounds in continuous mode
  let continuousBaseSecs = $state<number>(0);

  let snap = $derived($timerState);

  const currentContinuousElapsed = $derived(continuousBaseSecs + snap.elapsed_secs);

  const currentTotalTodaySecs = $derived(
    (snap.today_focus_secs ?? 0) +
      (timerMode === 'continuous'
        ? currentContinuousElapsed
        : snap.round_type === 'work'
          ? snap.elapsed_secs
          : 0)
  );

  function fmtTodayTime(secs: number): string {
    const mins = Math.floor(secs / 60);
    if (mins < 60) return `${mins}m`;
    const h = Math.floor(mins / 60);
    const m = mins % 60;
    return m === 0 ? `${h}h` : `${h}h ${m}m`;
  }

  const formattedTodayTime = $derived(fmtTodayTime(currentTotalTodaySecs));

  function roundColor(rt: string): string {
    if (timerMode === 'continuous') return 'var(--color-focus-round)';
    if (rt === 'work') return 'var(--color-focus-round)';
    if (rt === 'short-break') return 'var(--color-short-round)';
    return 'var(--color-long-round)';
  }

  function roundLabel(rt: string): string {
    if (timerMode === 'continuous') return m.round_label_continuous();
    if (rt === 'work') return m.round_label_work();
    if (rt === 'short-break') return m.round_label_short_break();
    return m.round_label_long_break();
  }

  async function handleModeChange(newMode: TimerMode) {
    if (timerMode === newMode) return;

    // Mode transition: When switching modes, any running/paused session >= 120s
    // is automatically saved to the database (via reset) before resetting to 00:00.
    const hasActiveSession =
      snap.is_running ||
      snap.is_paused ||
      (timerMode === 'continuous' && currentContinuousElapsed > 0) ||
      (timerMode === 'pomodoro' && snap.elapsed_secs > 0);

    if (hasActiveSession) {
      try {
        await timerReset();
      } catch (err) {
        console.error('Failed to reset timer on mode switch:', err);
      }
    }

    continuousBaseSecs = 0;
    if (typeof window !== 'undefined') {
      localStorage.setItem('pomotroid_timer_mode', newMode);
      localStorage.removeItem('pomotroid_continuous_base');
    }
    timerMode = newMode;
    try {
      const updated = await setSetting('timer_mode', newMode);
      settings.set(updated);
    } catch (err) {
      console.warn('Failed to save timer_mode to settings:', err);
    }
  }

  function handleContinuousReset() {
    continuousBaseSecs = 0;
    if (typeof window !== 'undefined') {
      localStorage.removeItem('pomotroid_continuous_base');
    }
  }

  async function handleRestartClick() {
    if (timerMode === 'continuous') {
      continuousBaseSecs = 0;
      if (typeof window !== 'undefined') {
        localStorage.removeItem('pomotroid_continuous_base');
      }
    }
    await timerRestartRound();
  }

  onMount(() => {
    const cleanups: UnlistenFn[] = [];

    if (typeof window !== 'undefined') {
      const savedMode = localStorage.getItem('pomotroid_timer_mode');
      if (savedMode === 'continuous' || savedMode === 'pomodoro') {
        timerMode = savedMode;
      }
      const savedBase = localStorage.getItem('pomotroid_continuous_base');
      if (savedBase) {
        continuousBaseSecs = parseInt(savedBase, 10) || 0;
      }
    }

    // Async setup: hydrate state and register event listeners.
    (async () => {
      const initial = await getTimerState();
      timerState.set(initial);

      cleanups.push(
        await onTimerTick(({ elapsed_secs, total_secs }) => {
          timerState.update((s) => ({
            ...s,
            elapsed_secs,
            total_secs,
            is_running: true,
            is_paused: false,
          }));
        }),
        await onTimerPaused(({ elapsed_secs }) => {
          timerState.update((s) => ({
            ...s,
            elapsed_secs,
            is_running: false,
            is_paused: true,
          }));
        }),
        await onTimerResumed(({ elapsed_secs }) => {
          timerState.update((s) => ({
            ...s,
            elapsed_secs,
            is_running: true,
            is_paused: false,
          }));
        }),
        await onRoundChange(async (nextSnap) => {
          if (timerMode === 'continuous') {
            // In continuous mode, if the server reached total duration and shifted to break,
            // the completed work session has already been saved to SQLite.
            // Accumulate duration and immediately skip break to continue focus uninterrupted.
            if (nextSnap.round_type !== 'work') {
              const finishedDuration = nextSnap.previous_round_type === 'work' ? (snap.total_secs || 1500) : 0;
              continuousBaseSecs += finishedDuration;
              if (typeof window !== 'undefined') {
                localStorage.setItem('pomotroid_continuous_base', String(continuousBaseSecs));
              }
              if ($settings.notifications_enabled) {
                const title = m.notification_continuous_title();
                const body = m.notification_continuous_body();
                notificationShow(title, body).catch(() => {});
              }
              try {
                await timerSkip();
                const latest = await getTimerState();
                if (!latest.is_running) {
                  await timerToggle();
                }
              } catch (err) {
                console.error('Error auto-skipping break in continuous mode:', err);
              }
              return;
            }
          }

          timerState.set(nextSnap);
          if ($settings.notifications_enabled && timerMode === 'pomodoro') {
            let title: string;
            let body: string;
            if (nextSnap.round_type === 'work') {
              const afterBreak =
                nextSnap.previous_round_type === 'short-break' ||
                nextSnap.previous_round_type === 'long-break';
              title = afterBreak ? m.notification_work_title() : m.notification_work_start_title();
              body = afterBreak ? m.notification_work_body() : m.notification_work_start_body();
            } else if (nextSnap.round_type === 'short-break') {
              title = m.notification_short_break_title();
              body = m.notification_short_break_body();
            } else {
              title = m.notification_long_break_title();
              body = m.notification_long_break_body();
            }
            notificationShow(title, body).catch(() => {});
          }
        }),
        await onTimerReset((resetSnap) => {
          timerState.set(resetSnap);
          continuousBaseSecs = 0;
          if (typeof window !== 'undefined') {
            localStorage.removeItem('pomotroid_continuous_base');
          }
        })
      );
    })();

    return () => {
      for (const unlisten of cleanups) unlisten();
    };
  });
</script>

<div class="timer-outer" class:compact={isCompact}>
  <div class="timer" style="zoom: {uiScale}">
    {#if !isCompact}
      <div class="mode-selector-wrapper">
        <div class="mode-pill-group" role="radiogroup" aria-label="Timer Mode">
          <Tooltip text={m.tooltip_timer_mode_pomodoro()}>
            <button
              type="button"
              class="mode-pill"
              class:active={timerMode === 'pomodoro'}
              onclick={() => handleModeChange('pomodoro')}
              role="radio"
              aria-checked={timerMode === 'pomodoro'}
            >
              {m.timer_mode_pomodoro()}
            </button>
          </Tooltip>
          <Tooltip text={m.tooltip_timer_mode_continuous()}>
            <button
              type="button"
              class="mode-pill"
              class:active={timerMode === 'continuous'}
              onclick={() => handleModeChange('continuous')}
              role="radio"
              aria-checked={timerMode === 'continuous'}
            >
              {m.timer_mode_continuous()}
            </button>
          </Tooltip>
        </div>
      </div>
      <div class="selectors-row">
        <PresetSelector />
        <TaskSelector />
      </div>
    {/if}

    <!-- Dial + display stacked (display centered over dial) -->
    <div class="dial-stack">
      <TimerDial
        {snap}
        countdown={$settings.dial_countdown}
        mode={timerMode}
        continuousElapsed={currentContinuousElapsed}
      />
      <TimerDisplay
        state={snap}
        mode={timerMode}
        continuousElapsed={currentContinuousElapsed}
      />
    </div>

    {#if !isCompact}
      <!-- Round type label sits below the dial as a normal flex child so it
           does not affect the dial-stack height used to centre TimerDisplay. -->
      <div class="round-label" style="color: {roundColor(snap.round_type)}">
        {roundLabel(snap.round_type)}
      </div>

      <div class="controls-wrapper">
        <!-- Back: restart current round -->
        <Tooltip text={m.tooltip_restart_round()}>
          <button class="btn-side" onclick={handleRestartClick} aria-label="Restart round">
            <svg width="18" height="18" viewBox="0 0 16 16">
              <polygon points="15,1 6,8 15,15" fill="currentColor" />
              <rect x="1" y="1" width="3" height="14" rx="1" fill="currentColor" />
            </svg>
          </button>
        </Tooltip>

        <!-- Play / Pause — icon fades when state changes -->
        <button
          class="play-pause"
          onclick={timerToggle}
          aria-label={snap.is_running ? 'Pause' : 'Play'}
        >
          {#key snap.is_running}
            <span class="icon" in:fade={{ duration: 120 }}>
              {#if snap.is_running}
                <svg width="20" height="20" viewBox="0 0 24 24">
                  <rect x="6" y="4" width="4" height="16" rx="1.5" fill="currentColor" />
                  <rect x="14" y="4" width="4" height="16" rx="1.5" fill="currentColor" />
                </svg>
              {:else}
                <svg width="20" height="20" viewBox="0 0 24 24">
                  <polygon points="7,4 19,12 7,20" fill="currentColor" />
                </svg>
              {/if}
            </span>
          {/key}
        </button>

        <!-- Skip: advance to next round in Pomodoro mode; placeholder in Continuous mode -->
        {#if timerMode === 'pomodoro'}
          <Tooltip text={m.tooltip_skip()}>
            <button class="btn-side" onclick={timerSkip} aria-label="Skip round">
              <svg width="18" height="18" viewBox="0 0 16 16">
                <polygon points="1,1 10,8 1,15" fill="currentColor" />
                <rect x="12" y="1" width="3" height="14" rx="1" fill="currentColor" />
              </svg>
            </button>
          </Tooltip>
        {:else}
          <div class="btn-placeholder" aria-hidden="true"></div>
        {/if}

        <TimerFooter {snap} mode={timerMode} onReset={handleContinuousReset} />
      </div>

      <!-- Today's total focus time -->
      <Tooltip text={m.tooltip_today_focus()}>
        <div class="today-focus">
          {m.timer_today_focus({ time: formattedTodayTime })}
        </div>
      </Tooltip>
    {/if}
  </div>

  {#if isCompact}
    <MiniControls mode={timerMode} />
  {/if}
</div>

<style>
  .today-focus {
    font-size: 0.73rem;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.04em;
    color: var(--color-foreground-darker);
    cursor: default;
    opacity: 0.85;
    margin-top: -2px;
    padding: 2px 8px;
    border-radius: 4px;
    transition:
      color var(--transition-default),
      opacity var(--transition-default);
  }

  .today-focus:hover {
    color: var(--color-foreground);
    opacity: 1;
  }

  .timer-outer {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }

  .timer {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
  }

  .dial-stack {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .controls-wrapper {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 4px 16px;
    align-items: center;
    justify-items: center;
    width: 100%;
    max-width: 250px;
  }

  .btn-side {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-foreground-darker, var(--color-foreground));
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 4px;
    transition:
      color var(--transition-default),
      background var(--transition-default);
  }

  .btn-side:hover {
    color: var(--color-foreground);
    background: var(--color-hover);
  }

  .play-pause {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-foreground);
    display: flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    border-radius: 50%;
    border: 2px solid var(--color-foreground-darker, var(--color-foreground));
    transition:
      color var(--transition-default),
      border-color var(--transition-default),
      background var(--transition-default);
    overflow: hidden; /* clip the fading icon within the circle */
  }

  .play-pause:hover {
    color: var(--color-accent);
    border-color: var(--color-accent);
    background: var(--color-hover);
  }

  .icon {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .round-label {
    font-size: 0.75rem;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    /* Collapse the gap above: the flex gap already provides spacing from the dial. */
    margin-top: -4px;
  }

  .selectors-row {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    margin-bottom: 2px;
    z-index: 35;
  }

  .mode-selector-wrapper {
    display: flex;
    justify-content: center;
    align-items: center;
  }

  .mode-pill-group {
    display: inline-flex;
    align-items: center;
    background: var(--color-background-light);
    padding: 2px;
    border-radius: 16px;
    gap: 2px;
  }

  .mode-pill {
    background: transparent;
    border: none;
    color: var(--color-foreground-darker, var(--color-foreground));
    font-size: 0.70rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    padding: 2px 10px;
    border-radius: 14px;
    cursor: pointer;
    transition:
      background var(--transition-default),
      color var(--transition-default);
  }

  .mode-pill:hover {
    color: var(--color-foreground);
  }

  .mode-pill.active {
    background: var(--color-focus-round);
    color: var(--color-background);
    font-weight: 700;
  }

  .btn-placeholder {
    width: 32px;
    height: 32px;
    visibility: hidden;
    pointer-events: none;
  }
</style>
