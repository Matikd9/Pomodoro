<script lang="ts">
  import type { DayStat, StreakInfo, TaskStat } from '$lib/types';
  import { obsidianExportWeekly } from '$lib/ipc';
  import * as m from '$paraglide/messages.js';
  import { getLocale } from '$paraglide/runtime.js';

  let {
    week,
    streak,
    tasks,
  }: {
    week: DayStat[] | null;
    streak: StreakInfo | null;
    tasks: TaskStat[] | null;
  } = $props();

  const CHART_H = 140; // px, max bar height
  const BAR_W = 52; // px per bar
  const BAR_GAP = 16; // px between bars
  const CHART_W = 7 * (BAR_W + BAR_GAP) - BAR_GAP; // 412px

  // Reactive to app language setting — updates when user changes language in Settings.
  const shortFmt = $derived(new Intl.DateTimeFormat(getLocale(), { weekday: 'short' }));
  const narrowFmt = $derived(new Intl.DateTimeFormat(getLocale(), { weekday: 'narrow' }));

  // Build a 7-day array (today and the previous 6 days), oldest first.
  const days = $derived.by(() => {
    const countByDate = new Map((week ?? []).map((d) => [d.date, d.rounds]));

    const today = new Date();
    today.setHours(0, 0, 0, 0);

    return Array.from({ length: 7 }, (_, i) => {
      const d = new Date(today);
      d.setDate(today.getDate() - (6 - i));
      const dateStr = [
        d.getFullYear(),
        String(d.getMonth() + 1).padStart(2, '0'),
        String(d.getDate()).padStart(2, '0'),
      ].join('-');
      return {
        date: dateStr,
        label: shortFmt.format(d),
        short: narrowFmt.format(d),
        rounds: countByDate.get(dateStr) ?? 0,
        isToday: i === 6,
      };
    });
  });

  function fmtRounds(r: number | undefined | null): string {
    if (r === undefined || r === null) return '—';
    return Number(r.toFixed(2)).toString();
  }

  function fmtTime(mins: number): string {
    if (mins < 60) return `${mins}m`;
    const h = Math.floor(mins / 60);
    const m = mins % 60;
    return m === 0 ? `${h}h` : `${h}h ${m}m`;
  }

  const maxRounds = $derived(Math.max(1, ...days.map((d) => d.rounds)));
  const totalWeek = $derived(days.reduce((s, d) => s + d.rounds, 0));
  const hasData = $derived(totalWeek > 0);

  let exportStatus = $state<'idle' | 'exporting' | 'success' | 'empty' | 'error'>('idle');
  let exportMessage = $state<string>('');

  async function handleObsidianExport() {
    exportStatus = 'exporting';
    try {
      const res = await obsidianExportWeekly(0);
      if (res.exported) {
        exportStatus = 'success';
        exportMessage = res.message;
      } else {
        exportStatus = 'empty';
        exportMessage = res.message;
      }
      setTimeout(() => {
        exportStatus = 'idle';
      }, 4000);
    } catch (err) {
      exportStatus = 'error';
      exportMessage = String(err);
      setTimeout(() => {
        exportStatus = 'idle';
      }, 4000);
    }
  }
</script>

<div class="view">
  <!-- Summary row -->
  <div class="summary">
    <div class="summary-item">
      <span class="summary-label">{m.stats_this_week()}</span>
      <span class="summary-value">{fmtRounds(totalWeek)} {m.stats_rounds().toLowerCase()}</span>
    </div>
    {#if streak}
      <div class="summary-item streak">
        <span class="summary-label">{m.stats_current_streak()}</span>
        <span class="summary-value">
          {#if streak.current > 0}
            <span class="flame" aria-hidden="true">🔥</span>{streak.current}
            {streak.current === 1 ? m.stats_day() : m.stats_days()}
          {:else}
            <span class="streak-none">{m.stats_no_active_streak()}</span>
          {/if}
        </span>
      </div>
    {/if}

    <div class="summary-actions">
      <button
        class="btn-obsidian"
        onclick={handleObsidianExport}
        disabled={exportStatus === 'exporting'}
        title={exportMessage || m.obsidian_export_tooltip()}
        aria-label={m.obsidian_export_button()}
      >
        <svg
          class="obsidian-icon"
          width="13"
          height="13"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
          <polyline points="14 2 14 8 20 8" />
          <line x1="16" y1="13" x2="8" y2="13" />
          <line x1="16" y1="17" x2="8" y2="17" />
          <polyline points="10 9 9 9 8 9" />
        </svg>
        {#if exportStatus === 'exporting'}
          <span>...</span>
        {:else if exportStatus === 'success'}
          <span class="export-success">✓ {m.obsidian_export_success()}</span>
        {:else if exportStatus === 'empty'}
          <span class="export-warn">⚠ {m.obsidian_export_no_data()}</span>
        {:else if exportStatus === 'error'}
          <span class="export-error">✕ Error</span>
        {:else}
          <span>{m.obsidian_export_button()}</span>
        {/if}
      </button>
    </div>
  </div>

  <!-- Bar chart -->
  <div class="chart-section">
    {#if !hasData}
      <div class="empty">
        <span>{m.stats_no_sessions_week()}</span>
      </div>
    {:else}
      <div class="chart-wrap">
        <svg
          width={CHART_W}
          height={CHART_H + 36}
          viewBox="0 0 {CHART_W} {CHART_H + 36}"
          class="chart"
        >
          {#each days as day, i}
            {@const barH = Math.max(
              day.rounds > 0 ? 4 : 0,
              Math.round((day.rounds / maxRounds) * CHART_H)
            )}
            {@const x = i * (BAR_W + BAR_GAP)}
            {@const y = CHART_H - barH}

            <!-- Bar -->
            <rect
              {x}
              {y}
              width={BAR_W}
              height={barH}
              rx="3"
              class="bar"
              class:bar-today={day.isToday}
              class:bar-empty={day.rounds === 0}
              style="--bar-delay: {i * 40}ms"
            />

            <!-- Round count label above bar -->
            {#if day.rounds > 0}
              <text x={x + BAR_W / 2} y={y - 5} text-anchor="middle" class="count-label"
                >{fmtRounds(day.rounds)}</text
              >
            {/if}

            <!-- Day label -->
            <text
              x={x + BAR_W / 2}
              y={CHART_H + 20}
              text-anchor="middle"
              class="day-label"
              class:day-label-today={day.isToday}>{day.short}</text
            >
          {/each}

          <!-- Baseline -->
          <line x1="0" y1={CHART_H} x2={CHART_W} y2={CHART_W} class="baseline" />
        </svg>
      </div>
    {/if}
  </div>

  <!-- Task breakdown -->
  {#if tasks && tasks.length > 0}
    <div class="tasks-section">
      <div class="tasks-header">
        <span class="tasks-title">{m.stats_tasks_breakdown()}</span>
      </div>

      <div class="tasks-list">
        {#each tasks as task}
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
    height: 100%;
    animation: app-fade-in 0.2s ease;
  }

  /* ── Summary row ─────────────────────────────────────────── */
  .summary {
    display: flex;
    align-items: center;
    gap: 32px;
    padding: 16px 32px 14px;
    border-bottom: 1px solid var(--color-separator);
    flex-shrink: 0;
  }

  .summary-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
  }

  .btn-obsidian {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 6px;
    background: color-mix(in oklch, var(--color-focus-round) 12%, transparent);
    border: 1px solid color-mix(in oklch, var(--color-focus-round) 25%, transparent);
    color: var(--color-foreground);
    font-size: 0.72rem;
    font-weight: 600;
    cursor: pointer;
    transition: all var(--transition-snappy);
    user-select: none;
  }

  .btn-obsidian:hover:not(:disabled) {
    background: color-mix(in oklch, var(--color-focus-round) 22%, transparent);
    border-color: color-mix(in oklch, var(--color-focus-round) 45%, transparent);
  }

  .btn-obsidian:disabled {
    opacity: 0.5;
    cursor: wait;
  }

  .obsidian-icon {
    color: var(--color-focus-round);
    flex-shrink: 0;
  }

  .export-success {
    color: #4ade80;
    font-weight: 600;
  }

  .export-warn {
    color: #fbbf24;
    font-weight: 500;
  }

  .export-error {
    color: #f87171;
    font-weight: 600;
  }

  .summary-item {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .summary-label {
    font-size: 0.67rem;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--color-foreground-darker);
  }

  .summary-value {
    font-size: 1.1rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--color-foreground);
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .streak-none {
    font-size: 0.9rem;
    font-weight: 400;
    color: var(--color-foreground-darker);
  }

  .flame {
    font-size: 1rem;
  }

  /* ── Bar chart ───────────────────────────────────────────── */
  .chart-section {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px 32px;
  }

  .empty {
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-foreground-darker);
    font-size: 0.85rem;
    font-style: italic;
    opacity: 0.7;
  }

  .chart-wrap {
    overflow: visible;
  }
  .chart {
    display: block;
    overflow: visible;
  }

  .bar {
    fill: color-mix(in oklch, var(--color-focus-round) 55%, var(--color-background-light));
    transform-origin: bottom;
    transform-box: fill-box;
    animation: bar-rise 0.45s cubic-bezier(0.22, 1, 0.36, 1) both;
    animation-delay: var(--bar-delay, 0ms);
  }

  @keyframes bar-rise {
    from {
      transform: scaleY(0);
      opacity: 0;
    }
    to {
      transform: scaleY(1);
      opacity: 1;
    }
  }

  .bar-today {
    fill: var(--color-focus-round);
  }

  .bar-empty {
    fill: color-mix(in oklch, var(--color-foreground) 6%, transparent);
    animation: none;
  }

  .count-label {
    fill: var(--color-foreground-darker);
    font-size: 10px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    animation: app-fade-in 0.3s ease both;
    animation-delay: 0.35s;
    cursor: default;
  }

  .day-label {
    fill: var(--color-foreground-darker);
    font-size: 10px;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    cursor: default;
  }

  .day-label-today {
    fill: var(--color-focus-round);
    font-weight: 700;
  }

  .baseline {
    stroke: var(--color-separator);
    stroke-width: 1;
  }

  /* ── Task breakdown ────────────────────────────────────────── */
  .tasks-section {
    border-top: 1px solid var(--color-separator);
    padding: 16px 32px 24px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .tasks-header {
    display: flex;
    align-items: baseline;
    gap: 12px;
  }

  .tasks-title {
    font-size: 0.68rem;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--color-foreground-darker);
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
