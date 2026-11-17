<script lang="ts">
  import { onMount } from 'svelte';
  import { sessionsAddManual, sessionsListForTask, sessionsTrim } from '$lib/ipc';
  import type { SessionItem } from '$lib/types';
  import * as m from '$paraglide/messages.js';

  let { taskName, onClose }: { taskName: string; onClose: () => void } = $props();

  const MIN_SECS = 120;
  const MAX_SECS = 12 * 60 * 60;

  type Tab = 'add' | 'remove';

  function localDateStr(d: Date): string {
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  }

  /** Local midnight of a YYYY-MM-DD string, plus `addDays` days (DST-safe). */
  function dayStart(dateStr: string, addDays = 0): Date {
    const [y, mo, d] = dateStr.split('-').map(Number);
    return new Date(y, mo - 1, d + addDays);
  }

  const today = localDateStr(new Date());

  let tab = $state<Tab>('add');
  let date = $state(today);
  let error = $state('');
  let notice = $state('');
  let busy = $state(false);

  // Add tab
  let startTime = $state('');
  let hours = $state(0);
  let minutes = $state(25);

  // Remove tab
  let sessions = $state<SessionItem[]>([]);
  let loadingSessions = $state(false);
  let selectedId = $state<number | null>(null);
  let removeMins = $state(0);
  let confirmingDelete = $state(false);

  const selected = $derived(sessions.find((s) => s.id === selectedId) ?? null);
  const removeSecs = $derived(Math.max(0, Math.round((Number(removeMins) || 0) * 60)));
  const remainingSecs = $derived(selected ? selected.duration_secs - removeSecs : 0);

  function fmtDur(secs: number): string {
    const mins = Math.max(0, Math.round(secs / 60));
    const h = Math.floor(mins / 60);
    const mm = mins % 60;
    if (h === 0) return `${mm}m`;
    if (mm === 0) return `${h}h`;
    return `${h}h ${mm}m`;
  }

  function fmtClock(ts: number): string {
    return new Date(ts * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }

  function errorText(e: unknown): string {
    const code = typeof e === 'string' ? e : '';
    switch (code) {
      case 'too_short':
        return m.adjust_err_too_short();
      case 'too_long':
        return m.adjust_err_too_long();
      case 'in_future':
        return m.adjust_err_in_future();
      default:
        return m.adjust_err_generic();
    }
  }

  function switchTab(next: Tab) {
    tab = next;
    error = '';
    notice = '';
    confirmingDelete = false;
  }

  async function loadSessions() {
    loadingSessions = true;
    try {
      const from = Math.floor(dayStart(date).getTime() / 1000);
      const to = Math.floor(dayStart(date, 1).getTime() / 1000);
      sessions = await sessionsListForTask(taskName, from, to);
      if (!sessions.some((s) => s.id === selectedId)) selectedId = null;
    } catch (e) {
      console.error('Failed to load sessions:', e);
      sessions = [];
      error = m.adjust_err_generic();
    } finally {
      loadingSessions = false;
    }
  }

  $effect(() => {
    // Reload whenever the Remove tab is shown or the date changes.
    if (tab === 'remove' && date) loadSessions();
  });

  async function submitAdd() {
    error = '';
    notice = '';
    const duration = Math.round(((Number(hours) || 0) * 60 + (Number(minutes) || 0)) * 60);
    if (duration < MIN_SECS) return void (error = m.adjust_err_too_short());
    if (duration > MAX_SECS) return void (error = m.adjust_err_too_long());

    const nowSecs = Math.floor(Date.now() / 1000);
    let startedAt: number;
    if (startTime) {
      const [hh, mi] = startTime.split(':').map(Number);
      const d = dayStart(date);
      d.setHours(hh, mi, 0, 0);
      startedAt = Math.floor(d.getTime() / 1000);
    } else {
      // Default: ends now for today, or at 23:59 for a past day.
      const end = date === today ? nowSecs : Math.floor(dayStart(date, 1).getTime() / 1000) - 60;
      startedAt = end - duration;
    }
    if (startedAt + duration > nowSecs) return void (error = m.adjust_err_in_future());

    busy = true;
    try {
      const res = await sessionsAddManual(taskName, startedAt, duration);
      notice =
        res.overlaps > 0
          ? m.adjust_overlap_warning({ count: String(res.overlaps) })
          : m.adjust_saved();
      startTime = '';
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function submitRemove() {
    error = '';
    notice = '';
    if (!selected) return void (error = m.adjust_err_select_session());
    if (removeSecs <= 0) return void (error = m.adjust_err_amount());
    if (remainingSecs < MIN_SECS && !confirmingDelete) {
      confirmingDelete = true;
      return;
    }
    confirmingDelete = false;
    busy = true;
    try {
      await sessionsTrim(selected.id, removeSecs);
      notice = m.adjust_saved();
      removeMins = 0;
      await loadSessions();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="backdrop"
  role="presentation"
  onclick={(e) => e.target === e.currentTarget && onClose()}
>
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    aria-label={m.adjust_title({ task: taskName })}
  >
    <div class="modal-header">
      <h3 title={taskName}>{m.adjust_title({ task: taskName })}</h3>
      <button class="close-btn" onclick={onClose} aria-label={m.adjust_close()}>✕</button>
    </div>

    <div class="tabs">
      <button class="tab" class:active={tab === 'add'} onclick={() => switchTab('add')}>
        {m.adjust_tab_add()}
      </button>
      <button class="tab" class:active={tab === 'remove'} onclick={() => switchTab('remove')}>
        {m.adjust_tab_remove()}
      </button>
    </div>

    <label class="field">
      <span>{m.adjust_date()}</span>
      <input
        type="date"
        bind:value={date}
        max={today}
        onchange={() => {
          error = '';
          notice = '';
          confirmingDelete = false;
        }}
      />
    </label>

    {#if tab === 'add'}
      <form
        class="body"
        onsubmit={(e) => {
          e.preventDefault();
          submitAdd();
        }}
      >
        <div class="field">
          <span>{m.adjust_duration()}</span>
          <div class="duration-row">
            <input type="number" min="0" max="12" bind:value={hours} />
            <span class="unit">{m.adjust_hours()}</span>
            <input type="number" min="0" max="59" bind:value={minutes} />
            <span class="unit">{m.adjust_minutes()}</span>
          </div>
        </div>
        <label class="field">
          <span>{m.adjust_start_time()}</span>
          <input type="time" bind:value={startTime} />
          <small>{m.adjust_start_hint()}</small>
        </label>
        <button type="submit" class="primary" disabled={busy}>{m.adjust_add_submit()}</button>
      </form>
    {:else}
      <form
        class="body"
        onsubmit={(e) => {
          e.preventDefault();
          submitRemove();
        }}
      >
        <small class="note">{m.adjust_in_progress_note()}</small>
        <div class="session-list">
          {#if loadingSessions}
            <span class="empty">…</span>
          {:else if sessions.length === 0}
            <span class="empty">{m.adjust_no_sessions()}</span>
          {:else}
            {#each sessions as s (s.id)}
              <label class="session-row" class:selected={selectedId === s.id}>
                <input
                  type="radio"
                  name="session"
                  value={s.id}
                  checked={selectedId === s.id}
                  onchange={() => {
                    selectedId = s.id;
                    confirmingDelete = false;
                  }}
                />
                <span class="session-time">{fmtClock(s.started_at)}–{fmtClock(s.ended_at)}</span>
                <span class="session-dur">{fmtDur(s.duration_secs)}</span>
                {#if s.manual}
                  <span class="manual-badge">{m.adjust_manual_badge()}</span>
                {/if}
              </label>
            {/each}
          {/if}
        </div>

        <label class="field">
          <span>{m.adjust_minutes_to_remove()}</span>
          <input
            type="number"
            min="1"
            bind:value={removeMins}
            oninput={() => (confirmingDelete = false)}
          />
        </label>

        {#if selected && removeSecs > 0}
          <div class="preview">
            {fmtDur(selected.duration_secs)} →
            {#if remainingSecs < MIN_SECS}
              <strong class="danger">{m.adjust_will_delete()}</strong>
            {:else}
              <strong>{fmtDur(remainingSecs)}</strong>
            {/if}
          </div>
        {/if}

        {#if confirmingDelete}
          <div class="confirm">
            <span>{m.adjust_delete_confirm({ time: fmtDur(Math.max(0, remainingSecs)) })}</span>
            <div class="confirm-actions">
              <button type="button" class="secondary" onclick={() => (confirmingDelete = false)}>
                {m.adjust_cancel()}
              </button>
              <button type="submit" class="primary danger-bg" disabled={busy}>
                {m.adjust_remove_submit()}
              </button>
            </div>
          </div>
        {:else}
          <button type="submit" class="primary" disabled={busy || !selected}>
            {m.adjust_remove_submit()}
          </button>
        {/if}
      </form>
    {/if}

    {#if error}
      <p class="msg error">{error}</p>
    {/if}
    {#if notice}
      <p class="msg notice">{notice}</p>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    padding: 16px;
  }

  .modal {
    width: 100%;
    max-width: 380px;
    max-height: 100%;
    overflow-y: auto;
    background: var(--color-background);
    border: 1px solid color-mix(in oklch, var(--color-foreground) 14%, transparent);
    border-radius: 10px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.3);
    color: var(--color-foreground);
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  h3 {
    margin: 0;
    font-size: 0.9rem;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--color-foreground-darker);
    cursor: pointer;
    font-size: 0.8rem;
  }

  .tabs {
    display: inline-flex;
    gap: 4px;
    background: color-mix(in oklch, var(--color-foreground) 5%, transparent);
    padding: 3px;
    border-radius: 7px;
    align-self: flex-start;
  }

  .tab {
    background: transparent;
    border: none;
    padding: 5px 12px;
    border-radius: 5px;
    font-size: 0.75rem;
    color: var(--color-foreground-darker);
    cursor: pointer;
  }

  .tab.active {
    background: var(--color-background-light);
    color: var(--color-foreground);
    font-weight: 600;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 0.75rem;
    color: var(--color-foreground-darker);
  }

  .field small,
  .note {
    font-size: 0.68rem;
    opacity: 0.8;
    color: var(--color-foreground-darker);
  }

  input[type='date'],
  input[type='time'],
  input[type='number'] {
    background: var(--color-background-light);
    border: 1px solid color-mix(in oklch, var(--color-foreground) 14%, transparent);
    border-radius: 6px;
    padding: 6px 8px;
    font-size: 0.8rem;
    color: var(--color-foreground);
    outline: none;
    color-scheme: light dark;
  }

  input:focus {
    border-color: var(--color-focus-round);
  }

  .duration-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .duration-row input {
    width: 64px;
  }

  .unit {
    font-size: 0.75rem;
  }

  .session-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 180px;
    overflow-y: auto;
  }

  .empty {
    font-size: 0.75rem;
    color: var(--color-foreground-darker);
    padding: 8px 0;
  }

  .session-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: 6px;
    border: 1px solid color-mix(in oklch, var(--color-foreground) 10%, transparent);
    background: var(--color-background-light);
    font-size: 0.78rem;
    cursor: pointer;
  }

  .session-row.selected {
    border-color: var(--color-focus-round);
  }

  .session-time {
    flex: 1;
  }

  .session-dur {
    font-weight: 600;
    color: var(--color-focus-round);
  }

  .manual-badge {
    font-size: 0.6rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 1px 5px;
    border-radius: 4px;
    background: color-mix(in oklch, var(--color-foreground) 10%, transparent);
    color: var(--color-foreground-darker);
  }

  .preview {
    font-size: 0.8rem;
  }

  .danger {
    color: #e05252;
  }

  .confirm {
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 0.76rem;
    padding: 8px;
    border-radius: 6px;
    background: color-mix(in oklch, #e05252 12%, transparent);
  }

  .confirm-actions {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }

  .primary,
  .secondary {
    padding: 7px 12px;
    border-radius: 6px;
    border: none;
    font-size: 0.76rem;
    font-weight: 600;
    cursor: pointer;
  }

  .primary {
    background: var(--color-focus-round);
    color: #fff;
  }

  .primary.danger-bg {
    background: #e05252;
  }

  .secondary {
    background: color-mix(in oklch, var(--color-foreground) 12%, transparent);
    color: var(--color-foreground);
  }

  .primary:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .msg {
    margin: 0;
    font-size: 0.75rem;
  }

  .msg.error {
    color: #e05252;
  }

  .msg.notice {
    color: var(--color-focus-round);
  }
</style>
