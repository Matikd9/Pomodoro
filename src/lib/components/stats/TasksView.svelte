<script lang="ts">
  import { onMount } from 'svelte';
  import {
    tasksGetSummary,
    tasksCreate,
    tasksRename,
    tasksToggleComplete,
    tasksDelete,
    tasksRestore,
    onRoundChange,
    onSessionsCleared,
  } from '$lib/ipc';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import type { TaskStatsSummary } from '$lib/types';
  import * as m from '$paraglide/messages.js';

  type Period = 'all' | 'month' | 'week' | 'today';
  type StatusFilter = 'active' | 'completed' | 'deleted';

  let tasks = $state<TaskStatsSummary[]>([]);
  let isLoading = $state(true);
  let selectedPeriod = $state<Period>('all');
  let selectedStatus = $state<StatusFilter>('active');
  let searchQuery = $state('');
  let newTaskInput = $state('');
  let isCreating = $state(false);

  // Inline editing state
  let editingTaskName = $state<string | null>(null);
  let editingValue = $state('');
  let isSubmittingEdit = $state(false);

  function focusOnMount(node: HTMLElement) {
    node.focus();
  }

  async function loadTasks() {
    try {
      tasks = await tasksGetSummary();
    } catch (e) {
      console.error('Failed to load tasks summary:', e);
    } finally {
      isLoading = false;
    }
  }

  onMount(() => {
    loadTasks();
    const cleanups: UnlistenFn[] = [];

    (async () => {
      try {
        cleanups.push(
          await onRoundChange(() => {
            loadTasks();
          }),
          await onSessionsCleared(() => {
            loadTasks();
          })
        );
      } catch (err) {
        console.warn('Failed to register listeners in TasksView:', err);
      }
    })();

    return () => {
      for (const fn of cleanups) fn();
    };
  });

  // Filter tasks by status and search
  const activeTasks = $derived(tasks.filter((t) => !t.deleted && !t.completed));
  const completedTasks = $derived(tasks.filter((t) => !t.deleted && t.completed));
  const deletedTasks = $derived(tasks.filter((t) => t.deleted));

  function getTaskSeconds(t: TaskStatsSummary, p: Period): number {
    const raw = t as any;
    switch (p) {
      case 'today':
        return Number(raw.today_secs ?? raw.todaySecs ?? 0);
      case 'week':
        return Number(raw.week_secs ?? raw.weekSecs ?? 0);
      case 'month':
        return Number(raw.month_secs ?? raw.monthSecs ?? 0);
      case 'all':
      default:
        return Number(raw.all_time_secs ?? raw.allTimeSecs ?? 0);
    }
  }

  function getTaskRounds(t: TaskStatsSummary, p: Period): number {
    const raw = t as any;
    switch (p) {
      case 'today':
        return Number(raw.today_rounds ?? raw.todayRounds ?? 0);
      case 'week':
        return Number(raw.week_rounds ?? raw.weekRounds ?? 0);
      case 'month':
        return Number(raw.month_rounds ?? raw.monthRounds ?? 0);
      case 'all':
      default:
        return Number(raw.all_time_rounds ?? raw.allTimeRounds ?? 0);
    }
  }

  const currentStatusList = $derived.by(() => {
    let list: TaskStatsSummary[] = [];
    if (selectedStatus === 'active') list = activeTasks;
    else if (selectedStatus === 'completed') list = completedTasks;
    else list = deletedTasks;

    const query = searchQuery.trim().toLowerCase();
    if (query) {
      list = list.filter((t) => t.name.toLowerCase().includes(query));
    }

    // Sort by seconds descending for the selected period, with 'General' always at top if present in active
    return [...list].sort((a, b) => {
      if (a.name === 'General') return -1;
      if (b.name === 'General') return 1;
      return getTaskSeconds(b, selectedPeriod) - getTaskSeconds(a, selectedPeriod);
    });
  });

  const maxSecondsInView = $derived.by(() => {
    let max = 0;
    for (const t of currentStatusList) {
      const s = getTaskSeconds(t, selectedPeriod);
      if (s > max) max = s;
    }
    return max > 0 ? max : 1;
  });

  function fmtTime(secs: number): string {
    const mins = Math.round(secs / 60);
    if (mins === 0) return '0m';
    const h = Math.floor(mins / 60);
    const m = mins % 60;
    if (h === 0) return `${m}m`;
    if (m === 0) return `${h}h`;
    return `${h}h ${m}m`;
  }

  function fmtRounds(n: number): string {
    return Number.isInteger(n) ? String(n) : n.toFixed(1);
  }

  async function handleCreateTask() {
    const clean = newTaskInput.trim();
    if (!clean || isCreating) return;
    isCreating = true;
    try {
      await tasksCreate(clean);
      newTaskInput = '';
      await loadTasks();
    } catch (e) {
      console.error('Failed to create task:', e);
    } finally {
      isCreating = false;
    }
  }

  function startEdit(taskName: string) {
    if (taskName === 'General') return;
    editingTaskName = taskName;
    editingValue = taskName;
  }

  function cancelEdit() {
    editingTaskName = null;
    editingValue = '';
  }

  async function saveEdit() {
    if (!editingTaskName || isSubmittingEdit) return;
    const clean = editingValue.trim();
    if (!clean || clean === editingTaskName) {
      cancelEdit();
      return;
    }
    isSubmittingEdit = true;
    try {
      const updated = await tasksRename(editingTaskName, clean);
      tasks = updated;
      cancelEdit();
    } catch (e) {
      console.error('Failed to rename task:', e);
    } finally {
      isSubmittingEdit = false;
    }
  }

  async function handleToggleComplete(taskName: string, completed: boolean) {
    try {
      await tasksToggleComplete(taskName, completed);
      await loadTasks();
    } catch (e) {
      console.error('Failed to toggle task completion:', e);
    }
  }

  async function handleDelete(taskName: string) {
    try {
      await tasksDelete(taskName);
      await loadTasks();
    } catch (e) {
      console.error('Failed to delete task:', e);
    }
  }

  async function handleRestore(taskName: string) {
    try {
      const updated = await tasksRestore(taskName);
      tasks = updated;
    } catch (e) {
      console.error('Failed to restore task:', e);
    }
  }
</script>

<div class="tasks-view">
  <!-- Top Bar: Search & Quick Add -->
  <div class="header-bar">
    <div class="search-wrap">
      <svg
        class="search-icon"
        width="13"
        height="13"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2.5"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <circle cx="11" cy="11" r="8" />
        <line x1="21" y1="21" x2="16.65" y2="16.65" />
      </svg>
      <input
        type="text"
        bind:value={searchQuery}
        placeholder={m.stats_tasks_search()}
        class="search-input"
      />
      {#if searchQuery}
        <button
          class="clear-search-btn"
          onclick={() => (searchQuery = '')}
          aria-label="Clear search"
        >
          ✕
        </button>
      {/if}
    </div>

    <!-- Quick Create Input -->
    <form class="create-wrap" onsubmit={(e) => { e.preventDefault(); handleCreateTask(); }}>
      <input
        type="text"
        bind:value={newTaskInput}
        placeholder={m.stats_tasks_new_placeholder()}
        maxlength="40"
        class="new-task-input"
      />
      <button
        type="submit"
        class="btn-create-task"
        disabled={!newTaskInput.trim() || isCreating}
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
        <span>{m.stats_tasks_create_btn()}</span>
      </button>
    </form>
  </div>

  <!-- Filters Row: Status Tabs + Period Selector -->
  <div class="filters-row">
    <!-- Status Tabs -->
    <div class="status-pills">
      <button
        class="status-pill"
        class:active={selectedStatus === 'active'}
        onclick={() => (selectedStatus = 'active')}
      >
        {m.stats_tasks_active()}
        <span class="count-badge">{activeTasks.length}</span>
      </button>
      <button
        class="status-pill"
        class:active={selectedStatus === 'completed'}
        onclick={() => (selectedStatus = 'completed')}
      >
        {m.stats_tasks_completed()}
        <span class="count-badge">{completedTasks.length}</span>
      </button>
      <button
        class="status-pill"
        class:active={selectedStatus === 'deleted'}
        onclick={() => (selectedStatus = 'deleted')}
      >
        {m.stats_tasks_deleted()}
        <span class="count-badge">{deletedTasks.length}</span>
      </button>
    </div>

    <!-- Period Selector -->
    <div class="period-pills">
      <button
        class="period-pill"
        class:active={selectedPeriod === 'all'}
        onclick={() => (selectedPeriod = 'all')}
      >
        {m.stats_period_all()}
      </button>
      <button
        class="period-pill"
        class:active={selectedPeriod === 'month'}
        onclick={() => (selectedPeriod = 'month')}
      >
        {m.stats_period_month()}
      </button>
      <button
        class="period-pill"
        class:active={selectedPeriod === 'week'}
        onclick={() => (selectedPeriod = 'week')}
      >
        {m.stats_period_week()}
      </button>
      <button
        class="period-pill"
        class:active={selectedPeriod === 'today'}
        onclick={() => (selectedPeriod = 'today')}
      >
        {m.stats_period_today()}
      </button>
    </div>
  </div>

  <!-- Task List -->
  <div class="tasks-scroll-list">
    {#if isLoading}
      <div class="empty-state">
        <span>{m.stats_loading()}</span>
      </div>
    {:else if currentStatusList.length === 0}
      <div class="empty-state">
        <svg
          width="24"
          height="24"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
          class="empty-icon"
        >
          <path d="M9 11l3 3L22 4" />
          <path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" />
        </svg>
        <p>{m.stats_tasks_empty()}</p>
      </div>
    {:else}
      {#each currentStatusList as task (task.name)}
        {@const secs = getTaskSeconds(task, selectedPeriod)}
        {@const rounds = getTaskRounds(task, selectedPeriod)}
        {@const percent = Math.round((secs / maxSecondsInView) * 100)}
        <div class="task-card" class:completed-task={task.completed} class:deleted-task={task.deleted}>
          <!-- Relative Progress Background Bar -->
          <div class="progress-bar-bg" style="width: {percent}%;"></div>

          <div class="task-card-inner">
            <!-- Left: Task Name or Edit Input -->
            <div class="task-main">
              {#if editingTaskName === task.name}
                <div class="inline-edit-wrap">
                  <input
                    type="text"
                    bind:value={editingValue}
                    maxlength="40"
                    class="edit-input"
                    onkeydown={(e) => {
                      if (e.key === 'Enter') saveEdit();
                      if (e.key === 'Escape') cancelEdit();
                    }}
                    use:focusOnMount
                  />
                  <button
                    class="btn-icon-action btn-save"
                    onclick={saveEdit}
                    title="Save"
                    aria-label="Save"
                  >
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round">
                      <polyline points="20 6 9 17 4 12" />
                    </svg>
                  </button>
                  <button
                    class="btn-icon-action btn-cancel"
                    onclick={cancelEdit}
                    title="Cancel"
                    aria-label="Cancel"
                  >
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round">
                      <line x1="18" y1="6" x2="6" y2="18" />
                      <line x1="6" y1="6" x2="18" y2="18" />
                    </svg>
                  </button>
                </div>
              {:else}
                <div class="task-title-wrap">
                  <span class="task-name" title={task.name}>{task.name}</span>
                  {#if task.name === 'General'}
                    <span class="default-badge">default</span>
                  {/if}
                </div>
              {/if}
            </div>

            <!-- Center/Right: Study Stats -->
            <div class="task-stats">
              <span class="focus-time-badge">{fmtTime(secs)}</span>
              <span class="rounds-badge">
                {fmtRounds(rounds)} {m.stats_rounds().toLowerCase()}
              </span>
            </div>

            <!-- Action Buttons -->
            <div class="task-actions">
              {#if selectedStatus === 'active'}
                <!-- Active Actions -->
                {#if task.name !== 'General'}
                  <button
                    class="action-btn complete-btn"
                    onclick={() => handleToggleComplete(task.name, true)}
                    title={m.stats_tasks_complete_btn()}
                    aria-label={m.stats_tasks_complete_btn()}
                  >
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round">
                      <polyline points="20 6 9 17 4 12" />
                    </svg>
                  </button>
                  <button
                    class="action-btn edit-btn"
                    onclick={() => startEdit(task.name)}
                    title={m.stats_tasks_edit_btn()}
                    aria-label={m.stats_tasks_edit_btn()}
                  >
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M12 20h9" />
                      <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" />
                    </svg>
                  </button>
                  <button
                    class="action-btn delete-btn"
                    onclick={() => handleDelete(task.name)}
                    title={m.stats_tasks_delete_btn()}
                    aria-label={m.stats_tasks_delete_btn()}
                  >
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                      <polyline points="3 6 5 6 21 6" />
                      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                    </svg>
                  </button>
                {/if}
              {:else if selectedStatus === 'completed'}
                <!-- Completed Actions -->
                <button
                  class="action-btn restore-btn"
                  onclick={() => handleToggleComplete(task.name, false)}
                  title={m.stats_tasks_restore_btn()}
                  aria-label={m.stats_tasks_restore_btn()}
                >
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
                    <polyline points="1 4 1 10 7 10" />
                    <path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10" />
                  </svg>
                </button>
                {#if task.name !== 'General'}
                  <button
                    class="action-btn edit-btn"
                    onclick={() => startEdit(task.name)}
                    title={m.stats_tasks_edit_btn()}
                    aria-label={m.stats_tasks_edit_btn()}
                  >
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M12 20h9" />
                      <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" />
                    </svg>
                  </button>
                  <button
                    class="action-btn delete-btn"
                    onclick={() => handleDelete(task.name)}
                    title={m.stats_tasks_delete_btn()}
                    aria-label={m.stats_tasks_delete_btn()}
                  >
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                      <polyline points="3 6 5 6 21 6" />
                      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                    </svg>
                  </button>
                {/if}
              {:else}
                <!-- Deleted Actions -->
                <button
                  class="action-btn restore-btn"
                  onclick={() => handleRestore(task.name)}
                  title={m.stats_tasks_restore_btn()}
                  aria-label={m.stats_tasks_restore_btn()}
                >
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
                    <polyline points="1 4 1 10 7 10" />
                    <path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10" />
                  </svg>
                  <span class="btn-text-label">{m.stats_tasks_restore_btn()}</span>
                </button>
              {/if}
            </div>
          </div>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .tasks-view {
    display: flex;
    flex-direction: column;
    padding: 16px 24px;
    gap: 14px;
    flex: 1;
    min-height: 100%;
    animation: tasks-fade-in 0.15s ease-out;
    box-sizing: border-box;
  }

  @keyframes tasks-fade-in {
    from {
      opacity: 0;
      transform: translateY(3px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  /* ── Header Bar ────────────────────────────────────────── */
  .header-bar {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
    align-items: center;
  }

  .search-wrap {
    position: relative;
    display: flex;
    align-items: center;
    flex: 1;
    min-width: 170px;
  }

  .search-icon {
    position: absolute;
    left: 9px;
    color: var(--color-foreground-darker);
    pointer-events: none;
    opacity: 0.7;
  }

  .search-input {
    width: 100%;
    background: var(--color-background-light);
    border: 1px solid color-mix(in oklch, var(--color-foreground) 14%, transparent);
    border-radius: 6px;
    padding: 7px 26px 7px 28px;
    font-size: 0.78rem;
    color: var(--color-foreground);
    outline: none;
    transition: border-color var(--transition-snappy);
    box-sizing: border-box;
  }

  .search-input:focus {
    border-color: var(--color-focus-round);
  }

  .search-input::placeholder {
    color: var(--color-foreground-darker);
    opacity: 0.6;
  }

  .clear-search-btn {
    position: absolute;
    right: 8px;
    background: none;
    border: none;
    color: var(--color-foreground-darker);
    font-size: 0.7rem;
    cursor: pointer;
    padding: 2px 4px;
    opacity: 0.7;
  }

  .clear-search-btn:hover {
    opacity: 1;
  }

  .create-wrap {
    display: flex;
    gap: 6px;
    align-items: center;
    flex: 1.2;
    min-width: 220px;
  }

  .new-task-input {
    flex: 1;
    background: var(--color-background-light);
    border: 1px solid color-mix(in oklch, var(--color-foreground) 14%, transparent);
    border-radius: 6px;
    padding: 7px 10px;
    font-size: 0.78rem;
    color: var(--color-foreground);
    outline: none;
    transition: border-color var(--transition-snappy);
    box-sizing: border-box;
  }

  .new-task-input:focus {
    border-color: var(--color-focus-round);
  }

  .new-task-input::placeholder {
    color: var(--color-foreground-darker);
    opacity: 0.6;
  }

  .btn-create-task {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 7px 12px;
    border-radius: 6px;
    background: var(--color-focus-round);
    color: #fff;
    border: none;
    font-size: 0.75rem;
    font-weight: 600;
    cursor: pointer;
    transition: opacity var(--transition-snappy);
    white-space: nowrap;
  }

  .btn-create-task:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .btn-create-task:not(:disabled):hover {
    opacity: 0.9;
  }

  /* ── Filters Row ───────────────────────────────────────── */
  .filters-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
    padding-bottom: 2px;
    border-bottom: 1px solid color-mix(in oklch, var(--color-foreground) 8%, transparent);
  }

  .status-pills {
    display: inline-flex;
    gap: 4px;
    background: color-mix(in oklch, var(--color-foreground) 5%, transparent);
    padding: 3px;
    border-radius: 7px;
  }

  .status-pill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: transparent;
    border: none;
    padding: 5px 10px;
    border-radius: 5px;
    font-size: 0.74rem;
    font-weight: 500;
    color: var(--color-foreground-darker);
    cursor: pointer;
    transition: all var(--transition-snappy);
  }

  .status-pill:hover {
    color: var(--color-foreground);
  }

  .status-pill.active {
    background: var(--color-background-light);
    color: var(--color-foreground);
    font-weight: 600;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  }

  .count-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 1px 5px;
    border-radius: 9999px;
    font-size: 0.65rem;
    font-weight: 600;
    background: color-mix(in oklch, var(--color-foreground) 10%, transparent);
    color: var(--color-foreground-darker);
  }

  .status-pill.active .count-badge {
    background: color-mix(in oklch, var(--color-focus-round) 18%, transparent);
    color: var(--color-focus-round);
  }

  .period-pills {
    display: inline-flex;
    gap: 2px;
  }

  .period-pill {
    background: transparent;
    border: 1px solid transparent;
    padding: 4px 8px;
    border-radius: 5px;
    font-size: 0.7rem;
    font-weight: 500;
    color: var(--color-foreground-darker);
    cursor: pointer;
    transition: all var(--transition-snappy);
  }

  .period-pill:hover {
    color: var(--color-foreground);
  }

  .period-pill.active {
    background: color-mix(in oklch, var(--color-focus-round) 12%, transparent);
    border-color: color-mix(in oklch, var(--color-focus-round) 25%, transparent);
    color: var(--color-focus-round);
    font-weight: 600;
  }

  /* ── Tasks Scroll List ─────────────────────────────────── */
  .tasks-scroll-list {
    display: flex;
    flex-direction: column;
    gap: 7px;
    flex: 1;
    overflow-y: auto;
    padding-right: 4px;
    margin-top: 4px;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 48px 0;
    color: var(--color-foreground-darker);
    font-size: 0.78rem;
  }

  .empty-icon {
    opacity: 0.4;
  }

  /* ── Task Card ─────────────────────────────────────────── */
  .task-card {
    position: relative;
    border-radius: 7px;
    background: var(--color-background-light);
    border: 1px solid color-mix(in oklch, var(--color-foreground) 10%, transparent);
    overflow: hidden;
    transition: border-color var(--transition-snappy), transform var(--transition-snappy);
  }

  .task-card:hover {
    border-color: color-mix(in oklch, var(--color-foreground) 20%, transparent);
  }

  .progress-bar-bg {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    background: color-mix(in oklch, var(--color-focus-round) 8%, transparent);
    pointer-events: none;
    transition: width 0.3s ease;
  }

  .task-card-inner {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    gap: 12px;
  }

  .task-main {
    flex: 1;
    min-width: 0;
  }

  .task-title-wrap {
    display: flex;
    align-items: center;
    gap: 6px;
    overflow: hidden;
  }

  .task-name {
    font-size: 0.82rem;
    font-weight: 600;
    color: var(--color-foreground);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .completed-task .task-name {
    text-decoration: line-through;
    opacity: 0.7;
  }

  .deleted-task .task-name {
    opacity: 0.75;
  }

  .default-badge {
    font-size: 0.62rem;
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 1px 5px;
    border-radius: 4px;
    background: color-mix(in oklch, var(--color-foreground) 10%, transparent);
    color: var(--color-foreground-darker);
    flex-shrink: 0;
  }

  .inline-edit-wrap {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .edit-input {
    background: var(--color-background);
    border: 1px solid var(--color-focus-round);
    border-radius: 4px;
    padding: 4px 7px;
    font-size: 0.8rem;
    color: var(--color-foreground);
    outline: none;
    min-width: 140px;
  }

  .btn-icon-action {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: 4px;
    border: none;
    cursor: pointer;
    transition: opacity var(--transition-snappy);
  }

  .btn-save {
    background: var(--color-focus-round);
    color: #fff;
  }

  .btn-cancel {
    background: color-mix(in oklch, var(--color-foreground) 12%, transparent);
    color: var(--color-foreground-darker);
  }

  /* ── Stats Meta ────────────────────────────────────────── */
  .task-stats {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-shrink: 0;
  }

  .focus-time-badge {
    font-size: 0.85rem;
    font-weight: 700;
    color: var(--color-focus-round);
    letter-spacing: -0.01em;
  }

  .rounds-badge {
    font-size: 0.72rem;
    color: var(--color-foreground-darker);
  }

  /* ── Actions ───────────────────────────────────────────── */
  .task-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }

  .action-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    width: 26px;
    height: 26px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 5px;
    cursor: pointer;
    color: var(--color-foreground-darker);
    transition: all var(--transition-snappy);
    padding: 0;
  }

  .action-btn:hover {
    color: var(--color-foreground);
    background: color-mix(in oklch, var(--color-foreground) 8%, transparent);
  }

  .complete-btn:hover {
    color: var(--color-focus-round);
    background: color-mix(in oklch, var(--color-focus-round) 15%, transparent);
  }

  .edit-btn:hover {
    color: var(--color-foreground);
    background: color-mix(in oklch, var(--color-foreground) 12%, transparent);
  }

  .delete-btn:hover {
    color: #e05252;
    background: color-mix(in oklch, #e05252 15%, transparent);
  }

  .restore-btn {
    width: auto;
    padding: 3px 8px;
    font-size: 0.72rem;
    font-weight: 500;
    border-color: color-mix(in oklch, var(--color-focus-round) 30%, transparent);
    color: var(--color-focus-round);
    background: color-mix(in oklch, var(--color-focus-round) 10%, transparent);
  }

  .restore-btn:hover {
    background: var(--color-focus-round);
    color: #fff;
  }

  .btn-text-label {
    white-space: nowrap;
  }
</style>
