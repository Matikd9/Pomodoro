<script lang="ts">
  // SVG arc dial showing timer progress.
  // Replicates the original Pomotroid dial: fills from 0% to 100% as time elapses.
  // In continuous mode: fills cyclically every 60 minutes with focus color.
  // Uses Svelte tweened store for smooth animation.
  import { tweened } from 'svelte/motion';
  import { cubicOut } from 'svelte/easing';
  import type { TimerState, TimerMode } from '$lib/types';

  interface Props {
    snap: TimerState;
    countdown?: boolean;
    mode?: TimerMode;
    continuousElapsed?: number;
  }

  let { snap, countdown = false, mode = 'pomodoro', continuousElapsed }: Props = $props();

  // SVG constants (matching original Pomotroid geometry)
  const CIRCUMFERENCE = 691.15; // 2π × 110 ≈ 691.15

  // Tweened offset: starts at full circumference (invisible), animates toward 0 (full arc).
  const dashOffset = tweened(CIRCUMFERENCE, { duration: 800, easing: cubicOut });

  // Round-type → CSS custom property for stroke color.
  function strokeColor(rt: string, currentMode: TimerMode): string {
    if (currentMode === 'continuous') return 'var(--color-focus-round)';
    if (rt === 'work') return 'var(--color-focus-round)';
    if (rt === 'short-break') return 'var(--color-short-round)';
    return 'var(--color-long-round)';
  }

  // Track previous round and mode to detect changes and snap the animation.
  let prevRound = $state<string>('');
  let prevMode = $state<TimerMode>('pomodoro');

  $effect(() => {
    const isCont = mode === 'continuous';
    const rt = snap.round_type;

    let progress: number;
    if (isCont) {
      const elapsed = continuousElapsed ?? snap.elapsed_secs;
      progress = (elapsed % 3600) / 3600;
    } else {
      progress = snap.total_secs > 0 ? snap.elapsed_secs / snap.total_secs : 0;
    }

    // In continuous mode, arc grows from empty → full (elapsed mode).
    // In pomodoro mode, respects countdown setting.
    const isCountdown = !isCont && countdown;
    const target = isCountdown ? CIRCUMFERENCE * progress : CIRCUMFERENCE * (1 - progress);
    const startOffset = isCountdown ? 0 : CIRCUMFERENCE;

    // On round or mode change: snap to start position if fresh start
    if (rt !== prevRound || mode !== prevMode) {
      if (snap.elapsed_secs === 0 && (!isCont || (continuousElapsed ?? 0) === 0)) {
        dashOffset.set(startOffset, { duration: 0 });
      } else {
        dashOffset.set(target);
      }
      prevRound = rt;
      prevMode = mode;
    } else {
      dashOffset.set(target);
    }
  });
</script>

<svg class="dial" viewBox="0 0 230 230" aria-hidden="true">
  <!-- Background track -->
  <path
    class="track"
    d="M115,5c60.8,0,110,49.2,110,110s-49.2,110-110,110S5,175.8,5,115S54.2,5,115,5"
    fill="none"
    stroke="var(--color-background-light)"
    stroke-width="2"
  />
  <!-- Progress arc -->
  <path
    class="progress"
    d="M115,5c60.8,0,110,49.2,110,110s-49.2,110-110,110S5,175.8,5,115S54.2,5,115,5"
    fill="none"
    stroke={strokeColor(snap.round_type, mode)}
    stroke-width="10"
    stroke-linecap="round"
    stroke-dasharray={CIRCUMFERENCE}
    stroke-dashoffset={$dashOffset}
  />
</svg>

<style>
  .dial {
    width: 220px;
    height: 220px;
    display: block;
  }
</style>
