<script lang="ts">
  import { onMount } from 'svelte';
  import { timerState } from '$lib/stores/timer';
  import { timerSetTask, tasksList, tasksCreate, tasksToggleComplete, tasksDelete, normalizeTask } from '$lib/ipc';
  import type { TaskItem } from '$lib/types';
  import * as m from '$paraglide/messages.js';

  let isOpen = $state(false);
  let tasks = $state<TaskItem[]>([{ name: 'General', completed: false }]);
  let newTaskInput = $state('');
  let popoverEl = $state<HTMLElement | null>(null);
  let inputEl = $state<HTMLInputElement | null>(null);
  let showCompleted = $state(false);

  const currentTask = $derived($timerState.current_task || 'General');
  const pendingTasks = $derived(tasks.filter((t) => !t.completed));
  const completedTasks = $derived(tasks.filter((t) => t.completed));

  async function loadTasks() {
    try {
      const list = await tasksList();
      if (list && list.length > 0) {
        tasks = list.map(normalizeTask);
      }
    } catch (e) {
      console.error('Failed to load tasks:', e);
    }
  }

  onMount(() => {
    loadTasks();

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

  async function selectTask(task: string) {
    try {
      const snap = await timerSetTask(task);
      timerState.set(snap);
      isOpen = false;
    } catch (e) {
      console.error('Failed to set task:', e);
    }
  }

  async function handleCreateTask() {
    const trimmed = newTaskInput.trim();
    if (!trimmed) return;
    try {
      const updated = await tasksCreate(trimmed);
      tasks = updated.map(normalizeTask);
      newTaskInput = '';
      await selectTask(trimmed);
    } catch (e) {
      console.error('Failed to create task:', e);
    }
  }

  async function handleCompleteTask(taskName: string, e?: MouseEvent) {
    e?.stopPropagation();
    try {
      const updated = await tasksToggleComplete(taskName, true);
      tasks = updated.map(normalizeTask);
      if (currentTask === taskName) {
        await selectTask('General');
      }
    } catch (err) {
      console.error('Failed to complete task:', err);
    }
  }

  async function handleRestoreTask(taskName: string, e?: MouseEvent) {
    e?.stopPropagation();
    try {
      const updated = await tasksToggleComplete(taskName, false);
      tasks = updated.map(normalizeTask);
    } catch (err) {
      console.error('Failed to restore task:', err);
    }
  }

  async function handleDeleteTask(taskName: string, e?: MouseEvent) {
    e?.stopPropagation();
    try {
      const updated = await tasksDelete(taskName);
      tasks = updated.map(normalizeTask);
      if (currentTask === taskName) {
        await selectTask('General');
      }
    } catch (err) {
      console.error('Failed to delete task:', err);
    }
  }

  function handleInputKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      handleCreateTask();
    }
  }

  function toggleOpen(e: MouseEvent) {
    e.stopPropagation();
    isOpen = !isOpen;
    if (isOpen) {
      loadTasks();
      setTimeout(() => inputEl?.focus(), 50);
    }
  }
</script>

<div class="task-selector-container" bind:this={popoverEl}>
  <div class="badge-wrapper">
    <!-- Task Badge / Button -->
    <button
      class="task-badge"
      class:active={isOpen}
      onclick={toggleOpen}
      title={m.task_change_tooltip()}
      aria-label={m.task_change_tooltip()}
    >
      <svg
        class="tag-icon"
        width="12"
        height="12"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path d="M20.59 13.41l-7.17 7.17a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z" />
        <line x1="7" y1="7" x2="7.01" y2="7" />
      </svg>
      <span class="task-name">{currentTask}</span>
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

    <!-- Quick Complete Button on main screen when non-General task is active -->
    {#if currentTask !== 'General'}
      <button
        class="btn-quick-complete"
        onclick={(e) => handleCompleteTask(currentTask, e)}
        title={m.task_complete_button()}
        aria-label={m.task_complete_button()}
      >
        <svg
          width="11"
          height="11"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <polyline points="20 6 9 17 4 12" />
        </svg>
      </button>
    {/if}
  </div>

  <!-- Dropdown popover -->
  {#if isOpen}
    <div class="task-popover" role="dialog" aria-modal="true">
      <div class="input-row">
        <input
          bind:this={inputEl}
          bind:value={newTaskInput}
          onkeydown={handleInputKeydown}
          type="text"
          placeholder={m.task_create_placeholder()}
          maxlength="40"
          class="task-input"
        />
        <button
          class="btn-create"
          onclick={handleCreateTask}
          disabled={!newTaskInput.trim()}
          title={m.task_create_button()}
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

      <!-- Pending Tasks List -->
      <div class="tasks-list">
        {#each pendingTasks as task, idx (task.name || idx)}
          <div class="task-row" class:selected={task.name === currentTask}>
            <button
              class="task-item-btn"
              onclick={() => selectTask(task.name)}
              title={task.name}
            >
              <span class="task-item-name">{task.name}</span>
              {#if task.name === currentTask}
                <span class="active-dot"></span>
              {/if}
            </button>
            {#if task.name !== 'General'}
              <button
                class="task-action-btn complete-btn"
                onclick={(e) => handleCompleteTask(task.name, e)}
                title={m.task_complete_button()}
                aria-label={m.task_complete_button()}
              >
                <svg
                  width="11"
                  height="11"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2.6"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <polyline points="20 6 9 17 4 12" />
                </svg>
              </button>
            {/if}
          </div>
        {/each}
      </div>

      <!-- Completed Tasks Collapsible Section -->
      {#if completedTasks.length > 0}
        <div class="completed-section">
          <button
            class="completed-header"
            onclick={() => (showCompleted = !showCompleted)}
          >
            <svg
              class="chevron-small"
              class:rotated={showCompleted}
              width="9"
              height="9"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <polyline points="9 18 15 12 9 6" />
            </svg>
            <span>{m.task_completed_section()} ({completedTasks.length})</span>
          </button>

          {#if showCompleted}
            <div class="completed-list">
              {#each completedTasks as task, idx (task.name || idx)}
                <div class="task-row completed-row">
                  <span class="task-item-name strike" title={task.name}>{task.name}</span>
                  <div class="completed-actions">
                    <button
                      class="task-action-btn restore-btn"
                      onclick={(e) => handleRestoreTask(task.name, e)}
                      title={m.task_uncomplete_button()}
                      aria-label={m.task_uncomplete_button()}
                    >
                      <svg
                        width="11"
                        height="11"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.4"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                      >
                        <polyline points="1 4 1 10 7 10" />
                        <path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10" />
                      </svg>
                    </button>
                    <button
                      class="task-action-btn delete-btn"
                      onclick={(e) => handleDeleteTask(task.name, e)}
                      title={m.task_delete_button()}
                      aria-label={m.task_delete_button()}
                    >
                      <svg
                        width="11"
                        height="11"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.4"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                      >
                        <polyline points="3 6 5 6 21 6" />
                        <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                      </svg>
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .task-selector-container {
    position: relative;
    display: flex;
    justify-content: center;
    align-items: center;
    z-index: 30;
    margin-bottom: 2px;
  }

  .badge-wrapper {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .task-badge {
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
    max-width: 175px;
    transition: all var(--transition-snappy);
    user-select: none;
  }

  .task-badge:hover,
  .task-badge.active {
    background: color-mix(in oklch, var(--color-foreground) 14%, transparent);
    border-color: color-mix(in oklch, var(--color-foreground) 24%, transparent);
  }

  .btn-quick-complete {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 9999px;
    background: color-mix(in oklch, var(--color-focus-round) 15%, transparent);
    border: 1px solid color-mix(in oklch, var(--color-focus-round) 30%, transparent);
    color: var(--color-focus-round);
    cursor: pointer;
    transition: all var(--transition-snappy);
    padding: 0;
  }

  .btn-quick-complete:hover {
    background: var(--color-focus-round);
    color: #fff;
    transform: scale(1.08);
  }

  .tag-icon {
    opacity: 0.65;
    flex-shrink: 0;
  }

  .task-name {
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

  .task-popover {
    position: absolute;
    top: calc(100% + 6px);
    width: 230px;
    background: var(--color-background-light);
    border: 1px solid color-mix(in oklch, var(--color-foreground) 16%, transparent);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    z-index: 50;
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
  }

  .task-input {
    flex: 1;
    background: var(--color-background);
    border: 1px solid color-mix(in oklch, var(--color-foreground) 15%, transparent);
    border-radius: 4px;
    padding: 5px 8px;
    font-size: 0.75rem;
    color: var(--color-foreground);
    outline: none;
    transition: border-color var(--transition-snappy);
  }

  .task-input:focus {
    border-color: var(--color-focus-round);
  }

  .task-input::placeholder {
    color: var(--color-foreground-darker);
    opacity: 0.6;
  }

  .btn-create {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    background: var(--color-focus-round);
    color: #fff;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    transition: opacity var(--transition-snappy);
  }

  .btn-create:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .btn-create:not(:disabled):hover {
    opacity: 0.9;
  }

  .tasks-list {
    display: flex;
    flex-direction: column;
    max-height: 140px;
    overflow-y: auto;
    gap: 2px;
  }

  .task-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 2px 4px;
    border-radius: 4px;
    transition: background var(--transition-snappy);
  }

  .task-row:hover {
    background: color-mix(in oklch, var(--color-foreground) 6%, transparent);
  }

  .task-row.selected {
    background: color-mix(in oklch, var(--color-focus-round) 12%, transparent);
  }

  .task-item-btn {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: transparent;
    border: none;
    color: var(--color-foreground);
    font-size: 0.74rem;
    text-align: left;
    cursor: pointer;
    padding: 3px 4px;
    min-width: 0;
  }

  .task-row.selected .task-item-btn {
    font-weight: 600;
    color: var(--color-focus-round);
  }

  .task-item-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .active-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--color-focus-round);
    flex-shrink: 0;
    margin-left: 4px;
  }

  .task-action-btn {
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
    opacity: 0.7;
    transition: all var(--transition-snappy);
    flex-shrink: 0;
  }

  .task-action-btn:hover {
    opacity: 1;
  }

  .complete-btn:hover {
    color: var(--color-focus-round);
    background: color-mix(in oklch, var(--color-focus-round) 15%, transparent);
  }

  .restore-btn:hover {
    color: var(--color-focus-round);
    background: color-mix(in oklch, var(--color-focus-round) 15%, transparent);
  }

  .delete-btn:hover {
    color: #e05252;
    background: color-mix(in oklch, #e05252 15%, transparent);
  }

  /* Completed Collapsible Section */
  .completed-section {
    border-top: 1px solid color-mix(in oklch, var(--color-foreground) 10%, transparent);
    padding-top: 4px;
    margin-top: 2px;
  }

  .completed-header {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    background: transparent;
    border: none;
    color: var(--color-foreground-darker);
    font-size: 0.68rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    cursor: pointer;
    padding: 3px 4px;
    border-radius: 3px;
    user-select: none;
  }

  .completed-header:hover {
    color: var(--color-foreground);
  }

  .chevron-small {
    transition: transform 0.15s ease;
  }

  .chevron-small.rotated {
    transform: rotate(90deg);
  }

  .completed-list {
    display: flex;
    flex-direction: column;
    max-height: 100px;
    overflow-y: auto;
    gap: 2px;
    margin-top: 2px;
  }

  .completed-row {
    padding: 2px 4px;
    opacity: 0.7;
  }

  .completed-row:hover {
    opacity: 1;
  }

  .strike {
    text-decoration: line-through;
    font-size: 0.72rem;
    flex: 1;
    min-width: 0;
  }

  .completed-actions {
    display: flex;
    align-items: center;
    gap: 2px;
  }
</style>
