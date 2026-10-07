<script lang="ts">
  import type { DailyStats } from '$lib/types';
  import { statsGetDailyByDate } from '$lib/ipc';
  import * as m from '$paraglide/messages.js';
  import { getLocale } from '$paraglide/runtime.js';

  let { today }: { today: DailyStats | null } = $props();

  const CHART_H = 80; // px, max bar height in the hourly chart
  const CHART_W = 744; // px, total SVG width for 24 bars
  const BAR_W = 22; // px per bar
  const BAR_GAP = 9; // px between bars

  function fmtTime(mins: number): string {
    if (mins < 60) return `${mins}m`;
    const h = Math.floor(mins / 60);
    const m = mins % 60;
    return m === 0 ? `${h}h` : `${h}h ${m}m`;
  }

  function fmtRate(rate: number | null): string {
    if (rate === null) return '—';
    return `${Math.round(rate * 100)}%`;
  }

  function fmtRounds(r: number | undefined | null): string {
    if (r === undefined || r === null) return '—';
    return Number(r.toFixed(2)).toString();
  }

  let selectedDate = $state<Date>(new Date());
  let currentDaily = $state<DailyStats | null>(null);
  let isLoading = $state(false);

  // Sync with prop when viewing today
  $effect(() => {
    if (isToday(selectedDate) && today) {
      currentDaily = today;
    }
  });

  function isToday(d: Date): boolean {
    const now = new Date();
    return (
      d.getFullYear() === now.getFullYear() &&
      d.getMonth() === now.getMonth() &&
      d.getDate() === now.getDate()
    );
  }

  const canGoForward = $derived(!isToday(selectedDate));

  function formatDateIso(d: Date): string {
    const y = d.getFullYear();
    const m = String(d.getMonth() + 1).padStart(2, '0');
    const day = String(d.getDate()).padStart(2, '0');
    return `${y}-${m}-${day}`;
  }

  // Format "Día, DD Mes AAAA" (ej. "Lunes, 28 Sep 2026")
  const dateFmt = $derived(
    new Intl.DateTimeFormat(getLocale(), {
      weekday: 'long',
      day: 'numeric',
      month: 'short',
      year: 'numeric',
    })
  );

  const formattedDate = $derived.by(() => {
    const raw = dateFmt.format(selectedDate);
    return raw.charAt(0).toUpperCase() + raw.slice(1);
  });

  async function loadDate(d: Date) {
    if (isToday(d) && today) {
      currentDaily = today;
      return;
    }
    isLoading = true;
    try {
      currentDaily = await statsGetDailyByDate(formatDateIso(d));
    } catch (e) {
      console.error('Failed to load stats for date:', e);
    } finally {
      isLoading = false;
    }
  }

  function prevDay() {
    const next = new Date(selectedDate);
    next.setDate(next.getDate() - 1);
    selectedDate = next;
    loadDate(next);
  }

  function nextDay() {
    if (!canGoForward) return;
    const next = new Date(selectedDate);
    next.setDate(next.getDate() + 1);
    selectedDate = next;
    loadDate(next);
  }

  const byHour = $derived(currentDaily?.by_hour ?? Array(24).fill(0));
  const maxHour = $derived(Math.max(1, ...byHour));
  const hasData = $derived(currentDaily !== null && currentDaily.rounds > 0);

  // Hour labels: show every 6 hours
  const hourLabels = [0, 6, 12, 18];
</script>

<div class="view">
  <!-- Date navigation header -->
  <div class="nav-bar">
    <button
      class="nav-btn"
      onclick={prevDay}
      aria-label={m.stats_prev_day()}
      title={m.stats_prev_day()}
    >
      <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
        <polyline
          points="9,2 4,7 9,12"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    </button>
    <div class="date-label-wrap">
      <span class="date-label">{formattedDate}</span>
      {#if isToday(selectedDate)}
        <span class="today-badge">{m.stats_tab_today()}</span>
      {/if}
    </div>
    <button
      class="nav-btn"
      onclick={nextDay}
      disabled={!canGoForward}
      aria-label={m.stats_next_day()}
      title={m.stats_next_day()}
    >
      <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
        <polyline
          points="5,2 10,7 5,12"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    </button>
  </div>

  <!-- Stat cards -->
  <div class="cards">
    <div class="card" style="--delay: 0ms">
      <span class="card-label">{m.stats_rounds()}</span>
      <span class="card-value">{currentDaily ? fmtRounds(currentDaily.rounds) : '—'}</span>
    </div>
    <div class="card-divider"></div>
    <div class="card" style="--delay: 60ms">
      <span class="card-label">{m.stats_focus_time()}</span>
      <span class="card-value">{currentDaily ? fmtTime(currentDaily.focus_mins) : '—'}</span>
    </div>
    <div class="card-divider"></div>
    <div class="card" style="--delay: 120ms">
      <span class="card-label">{m.stats_completion()}</span>
      <span class="card-value">{currentDaily ? fmtRate(currentDaily.completion_rate) : '—'}</span>
    </div>
  </div>

  <!-- Hourly breakdown -->
  <div class="chart-section">
    <div class="section-header">
      <span class="section-title">{m.stats_sessions_by_hour()}</span>
      {#if !hasData}
        <span class="empty-hint">
          {isToday(selectedDate) ? m.stats_no_sessions_today() : m.stats_no_sessions_date()}
        </span>
      {/if}
    </div>

    <div class="chart-wrap">
      <svg
        width={CHART_W}
        height={CHART_H + 28}
        viewBox="0 0 {CHART_W} {CHART_H + 28}"
        class="chart"
      >
        {#each byHour as count, h}
          {@const barH = Math.max(2, Math.round((count / maxHour) * CHART_H))}
          {@const x = h * (BAR_W + BAR_GAP)}
          {@const y = CHART_H - barH}

          <!-- Bar -->
          <rect
            {x}
            {y}
            width={BAR_W}
            height={barH}
            rx="2"
            class="bar"
            class:bar-empty={count === 0}
            style="--bar-scale: {barH / CHART_H}; --bar-delay: {h * 18}ms"
          >
            {#if count > 0}
              <title>{h}:00 – {fmtTime(Math.round(count))}</title>
            {/if}
          </rect>

          <!-- Hour label (every 6 hours) -->
          {#if hourLabels.includes(h)}
            <text x={x + BAR_W / 2} y={CHART_H + 18} text-anchor="middle" class="hour-label"
              >{h === 0 ? '12a' : h === 12 ? '12p' : h < 12 ? `${h}a` : `${h - 12}p`}</text
            >
          {/if}
        {/each}

        <!-- Baseline -->
        <line x1="0" y1={CHART_H} x2={CHART_W} y2={CHART_H} class="baseline" />
      </svg>
    </div>
  </div>

  <!-- Task breakdown -->
  {#if currentDaily && currentDaily.task_breakdown && currentDaily.task_breakdown.length > 0}
    <div class="tasks-section">
      <div class="section-header">
        <span class="section-title">{m.stats_tasks_breakdown()}</span>
      </div>

      <div class="tasks-list">
        {#each currentDaily.task_breakdown as task}
          {@const mins = Math.round(task.focus_secs / 60)}
          <div class="task-row">
            <div class="task-meta">
              <span class="task-name">{task.task_name}</span>
              <span class="task-rounds"
                >{fmtRounds(task.rounds)} {m.stats_rounds().toLowerCase()}</span
              >
            </div>
            <span class="task-time">{fmtTime(mins)}</span>
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>

<style>
  .view {
    display: flex;
    flex-direction: column;
    gap: 0;
    min-height: 100%;
    padding: 0;
    animation: app-fade-in 0.2s ease;
  }

  /* ── Date navigation ─────────────────────────────────────── */
  .nav-bar {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 10px 20px 8px;
    border-bottom: 1px solid var(--color-separator);
    flex-shrink: 0;
  }

  .nav-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: 4px;
    background: transparent;
    border: none;
    color: var(--color-foreground-darker);
    cursor: pointer;
    transition: all var(--transition-snappy);
  }

  .nav-btn:hover:not(:disabled) {
    background: var(--color-hover);
    color: var(--color-foreground);
  }

  .nav-btn:disabled {
    opacity: 0.25;
    cursor: default;
  }

  .date-label-wrap {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .date-label {
    font-size: 0.88rem;
    font-weight: 600;
    color: var(--color-foreground);
    letter-spacing: 0.01em;
  }

  .today-badge {
    font-size: 0.65rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 2px 6px;
    border-radius: 4px;
    background: color-mix(in oklch, var(--color-focus-round) 15%, transparent);
    color: var(--color-focus-round);
  }

  /* ── Stat cards ──────────────────────────────────────────── */
  .cards {
    display: flex;
    align-items: stretch;
    border-bottom: 1px solid var(--color-separator);
    flex-shrink: 0;
  }

  .card {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 20px 24px;
    animation: card-rise 0.35s cubic-bezier(0.22, 1, 0.36, 1) both;
    animation-delay: var(--delay, 0ms);
  }

  @keyframes card-rise {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .card-label {
    font-size: 0.68rem;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--color-foreground-darker);
  }

  .card-value {
    font-size: 2.4rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.02em;
    color: var(--color-foreground);
    line-height: 1;
  }

  .card-divider {
    width: 1px;
    background: var(--color-separator);
    align-self: stretch;
    margin: 12px 0;
  }

  /* ── Hourly chart section ────────────────────────────────── */
  .chart-section {
    flex: 1;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    padding: 16px 24px 14px;
    gap: 12px;
  }

  .section-header {
    display: flex;
    align-items: baseline;
    gap: 12px;
  }

  .section-title {
    font-size: 0.68rem;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--color-foreground-darker);
  }

  .empty-hint {
    font-size: 0.72rem;
    color: color-mix(in oklch, var(--color-foreground-darker) 60%, transparent);
    font-style: italic;
  }

  /* ── Hourly chart ────────────────────────────────────────── */
  .chart-wrap {
    overflow-x: auto;
  }

  .chart {
    display: block;
    overflow: visible;
  }

  .bar {
    fill: var(--color-focus-round);
    opacity: 0.85;
    transform-origin: bottom;
    transform-box: fill-box;
    animation: bar-rise 0.4s cubic-bezier(0.22, 1, 0.36, 1) both;
    animation-delay: var(--bar-delay, 0ms);
  }

  @keyframes bar-rise {
    from {
      transform: scaleY(0.05);
      opacity: 0;
    }
    to {
      transform: scaleY(1);
      opacity: 0.85;
    }
  }

  .bar-empty {
    fill: color-mix(in oklch, var(--color-foreground) 8%, transparent);
    animation: none;
  }

  .hour-label {
    fill: var(--color-foreground-darker);
    font-size: 9px;
    font-variant-numeric: tabular-nums;
    cursor: default;
  }

  .baseline {
    stroke: var(--color-separator);
    stroke-width: 1;
  }

  /* ── Task breakdown ────────────────────────────────────────── */
  .tasks-section {
    border-top: 1px solid var(--color-separator);
    padding: 16px 24px 20px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    flex-shrink: 0;
  }

  .tasks-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .task-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    background: color-mix(in oklch, var(--color-foreground) 4%, transparent);
    border-radius: 6px;
    border: 1px solid color-mix(in oklch, var(--color-foreground) 6%, transparent);
  }

  .task-meta {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }

  .task-name {
    font-size: 0.82rem;
    font-weight: 600;
    color: var(--color-foreground);
  }

  .task-rounds {
    font-size: 0.72rem;
    color: var(--color-foreground-darker);
  }

  .task-time {
    font-size: 0.82rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    color: var(--color-focus-round);
  }
</style>
