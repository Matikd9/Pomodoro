<script lang="ts">
  // Displays the timer:
  // - In Pomodoro mode: countdown remaining time (MM:SS).
  // - In Continuous mode: count-up stopwatch time (MM:SS or HH:MM:SS once >= 1 hour).
  import type { TimerState, TimerMode } from '$lib/types';

  interface Props {
    state: TimerState;
    mode?: TimerMode;
    continuousElapsed?: number;
  }

  let { state, mode = 'pomodoro', continuousElapsed }: Props = $props();

  // Pomodoro countdown
  let remaining = $derived(Math.max(0, state.total_secs - state.elapsed_secs));
  let pomoMinutes = $derived(Math.floor(remaining / 60));
  let pomoSeconds = $derived(remaining % 60);

  // Continuous count-up
  let contElapsed = $derived(continuousElapsed ?? state.elapsed_secs);
  let contHours = $derived(Math.floor(contElapsed / 3600));
  let contMinutes = $derived(Math.floor((contElapsed % 3600) / 60));
  let contSeconds = $derived(contElapsed % 60);

  let isContinuous = $derived(mode === 'continuous');
  let hasHours = $derived(isContinuous && contHours > 0);

  let display = $derived(
    isContinuous
      ? hasHours
        ? `${String(contHours).padStart(2, '0')}:${String(contMinutes).padStart(2, '0')}:${String(contSeconds).padStart(2, '0')}`
        : `${String(contMinutes).padStart(2, '0')}:${String(contSeconds).padStart(2, '0')}`
      : `${String(pomoMinutes).padStart(2, '0')}:${String(pomoSeconds).padStart(2, '0')}`
  );
</script>

<div class="display">
  <span class="time" class:has-hours={hasHours}>{display}</span>
</div>

<style>
  .display {
    /* Fill the dial-stack and flex-center the time. */
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
  }

  .time {
    font-family: 'Mona Sans Mono', monospace;
    font-size: 2.8rem;
    font-weight: 300;
    font-stretch: 85%;
    letter-spacing: -0.02em;
    color: var(--color-foreground);
    transition: font-size 0.2s ease;
  }

  .time.has-hours {
    font-size: 2.05rem;
    letter-spacing: -0.04em;
  }
</style>
