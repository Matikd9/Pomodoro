<script lang="ts">
  import '../../app.css';
  import { onMount } from 'svelte';
  import {
    getSettings,
    getThemes,
    onSettingsChanged,
    onThemesChanged,
    onRoundChange,
    onSessionsCleared,
    statsGetDetailed,
    statsGetHeatmap,
  } from '$lib/ipc';
  import { settings } from '$lib/stores/settings';
  import { applyTheme } from '$lib/stores/theme';
  import { setLocale } from '$lib/locale.svelte.js';
  import { resolveThemeName } from '$lib/utils/theme';
  import { isMac, isTauri } from '$lib/utils/platform';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import type { DetailedStats, HeatmapStats } from '$lib/types';
  import * as m from '$paraglide/messages.js';
  import { logInfo as info, logError } from '$lib/utils/log';

  import TasksView from '$lib/components/stats/TasksView.svelte';
  import DailyView from '$lib/components/stats/DailyView.svelte';
  import WeeklyView from '$lib/components/stats/WeeklyView.svelte';
  import YearlyView from '$lib/components/stats/YearlyView.svelte';

  type Tab = 'tasks' | 'today' | 'week' | 'alltime';

  let activeTab = $state<Tab>('today');
  let detailed = $state<DetailedStats | null>(null);
  let heatmap = $state<HeatmapStats | null>(null);
  let heatmapLoaded = $state(false);

  async function switchTab(tab: Tab) {
    activeTab = tab;
    if (tab === 'alltime' && !heatmapLoaded) {
      try {
        heatmap = await statsGetHeatmap();
        heatmapLoaded = true;
      } catch (e) {
        await logError(`[stats] failed to load heatmap: ${e}`);
      }
    }
  }

  async function close() {
    if (isTauri) {
      const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow');
      getCurrentWebviewWindow().close();
    } else {
      window.location.href = '/';
    }
  }

  onMount(() => {
    const cleanups: UnlistenFn[] = [];

    (async () => {
      try {
        const s = await getSettings();
        settings.set(s);
        setLocale(s.language);
        await info(`[stats] settings loaded, locale=${s.language}`);

        const themes = await getThemes();
        const osDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
        const activeTheme = themes.find((t) => t.name === resolveThemeName(s, osDark)) ?? themes[0];
        if (activeTheme) applyTheme(activeTheme);

        // Show window immediately after theme is applied
        if (isTauri) {
          const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow');
          await getCurrentWebviewWindow().show();
        }

        detailed = await statsGetDetailed();
        await info(`[stats] initialized, theme=${activeTheme?.name ?? 'none'}`);
      } catch (e) {
        await logError(`[stats] initialization failed: ${e}`);
        throw e;
      }

      cleanups.push(
        await onRoundChange(async () => {
          try {
            detailed = await statsGetDetailed();
            if (heatmapLoaded) heatmap = await statsGetHeatmap();
          } catch (e) {
            await logError(`[stats] failed to refresh stats after round change: ${e}`);
          }
        }),
        await onSessionsCleared(async () => {
          try {
            detailed = await statsGetDetailed();
            if (heatmapLoaded) heatmap = await statsGetHeatmap();
          } catch (e) {
            await logError(`[stats] failed to refresh stats after session clear: ${e}`);
          }
        }),
        await onSettingsChanged(async (updated) => {
          const prev = {
            mode: $settings.theme_mode,
            light: $settings.theme_light,
            dark: $settings.theme_dark,
            language: $settings.language,
          };
          settings.set(updated);
          if (updated.language !== prev.language) {
            setLocale(updated.language);
          }
          if (
            updated.theme_mode !== prev.mode ||
            updated.theme_light !== prev.light ||
            updated.theme_dark !== prev.dark
          ) {
            const allThemes = await getThemes();
            const dark = window.matchMedia('(prefers-color-scheme: dark)').matches;
            const t = allThemes.find((th) => th.name === resolveThemeName(updated, dark));
            if (t) applyTheme(t);
          }
        }),
        await onThemesChanged((updated) => {
          const dark = window.matchMedia('(prefers-color-scheme: dark)').matches;
          const current =
            updated.find((t) => t.name === resolveThemeName($settings, dark)) ?? updated[0];
          if (current) applyTheme(current);
        })
      );
    })();

    return () => {
      for (const fn of cleanups) fn();
    };
  });
</script>

<div class="window">
  <!-- Titlebar -->
  <nav class="titlebar" class:macos={isMac} data-tauri-drag-region>
    <span class="titlebar-label">{m.stats_title()}</span>
    {#if !isMac}
      <button class="btn-close" onclick={close} aria-label="Close">
        <svg width="12" height="12" viewBox="0 0 12 12">
          <line
            x1="1"
            y1="1"
            x2="11"
            y2="11"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
          />
          <line
            x1="11"
            y1="1"
            x2="1"
            y2="11"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
          />
        </svg>
      </button>
    {/if}
  </nav>

  <!-- Tab bar -->
  <div class="tabs">
    <button class="tab" class:active={activeTab === 'tasks'} onclick={() => switchTab('tasks')}
      >{m.stats_tab_tasks()}</button
    >
    <button class="tab" class:active={activeTab === 'today'} onclick={() => switchTab('today')}
      >{m.stats_tab_today()}</button
    >
    <button class="tab" class:active={activeTab === 'week'} onclick={() => switchTab('week')}
      >{m.stats_tab_week()}</button
    >
    <button class="tab" class:active={activeTab === 'alltime'} onclick={() => switchTab('alltime')}
      >{m.stats_tab_alltime()}</button
    >
  </div>

  <!-- Content -->
  <div class="content">
    {#if activeTab === 'tasks'}
      <TasksView />
    {:else if activeTab === 'today'}
      <DailyView today={detailed?.today ?? null} />
    {:else if activeTab === 'week'}
      <WeeklyView
        week={detailed?.week ?? null}
        streak={detailed?.streak ?? null}
        tasks={detailed?.week_tasks ?? null}
      />
    {:else}
      <YearlyView {heatmap} />
    {/if}
  </div>
</div>

<style>
  .window {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--color-background);
    color: var(--color-foreground);
    animation: app-fade-in 0.18s ease;
    overflow: hidden;
    cursor: default;
  }

  /* ── Titlebar ──────────────────────────────────────────── */
  .titlebar {
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
    flex-shrink: 0;
    border-bottom: 1px solid var(--color-separator);
  }

  .macos {
    padding-left: 72px;
  }

  .titlebar-label {
    font-size: 0.75rem;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--color-foreground-darker);
    pointer-events: none;
  }

  .btn-close {
    position: absolute;
    right: 8px;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-foreground-darker);
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 4px;
    transition:
      color 0.15s,
      background 0.15s;
  }

  .btn-close:hover {
    color: var(--color-background);
    background: var(--color-focus-round);
  }

  /* ── Tabs ──────────────────────────────────────────────── */
  .tabs {
    display: flex;
    gap: 0;
    border-bottom: 1px solid var(--color-separator);
    flex-shrink: 0;
    padding: 0 24px;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .tabs::-webkit-scrollbar {
    display: none;
  }

  .tab {
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    padding: 10px 20px;
    font-size: 0.78rem;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--color-foreground-darker);
    cursor: pointer;
    transition:
      color 0.15s,
      border-color 0.15s;
  }

  .tab:hover {
    color: var(--color-foreground);
  }

  .tab.active {
    color: var(--color-focus-round);
    border-bottom-color: var(--color-focus-round);
  }

  /* ── Content ───────────────────────────────────────────── */
  .content {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    display: flex;
    flex-direction: column;
  }

  @media (max-width: 600px), (pointer: coarse) {
    .titlebar {
      margin-top: max(env(safe-area-inset-top, 0px), 14px);
      height: 48px;
    }

    .btn-close {
      width: 44px;
      height: 44px;
      right: 12px;
      border-radius: 8px;
    }
  }
</style>
