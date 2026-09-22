<script lang="ts">
  import { onMount } from 'svelte';
  import { timerState } from '$lib/stores/timer';
  import { timerSetTask, tasksList, tasksCreate } from '$lib/ipc';
  import * as m from '$paraglide/messages.js';

  let isOpen = $state(false);
  let tasks = $state<string[]>(['General']);
  let newTaskInput = $state('');
  let popoverEl = $state<HTMLElement | null>(null);
  let inputEl = $state<HTMLInputElement | null>(null);

  const currentTask = $derived($timerState.current_task || 'General');

  async function loadTasks() {
    try {
      const list = await tasksList();
      if (list && list.length > 0) {
        tasks = list;
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
      tasks = updated;
      newTaskInput = '';
      await selectTask(trimmed);
    } catch (e) {
      console.error('Failed to create task:', e);
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

      <div class="tasks-list">
        {#each tasks as task}
          <button
            class="task-item"
            class:selected={task === currentTask}
            onclick={() => selectTask(task)}
          >
            <span class="task-item-name">{task}</span>
            {#if task === currentTask}
              <svg
                class="check-icon"
                width="12"
                height="12"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="3"
                stroke-linecap="round"
                stroke-linejoin="round"
              >
                <polyline points="20 6 9 17 4 12" />
              </svg>
            {/if}
          </button>
        {/each}
      </div>
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
    max-width: 190px;
    transition: all var(--transition-snappy);
    user-select: none;
  }

  .task-badge:hover,
  .task-badge.active {
    background: color-mix(in oklch, var(--color-foreground) 14%, transparent);
    border-color: color-mix(in oklch, var(--color-foreground) 24%, transparent);
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
    width: 220px;
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

  .task-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 5px 8px;
    border-radius: 4px;
    background: transparent;
    border: none;
    color: var(--color-foreground);
    font-size: 0.73rem;
    text-align: left;
    cursor: pointer;
    transition: background var(--transition-snappy);
  }

  .task-item:hover {
    background: color-mix(in oklch, var(--color-foreground) 8%, transparent);
  }

  .task-item.selected {
    font-weight: 600;
    color: var(--color-focus-round);
    background: color-mix(in oklch, var(--color-focus-round) 12%, transparent);
  }

  .task-item-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .check-icon {
    flex-shrink: 0;
    color: var(--color-focus-round);
  }
</style>
