<script lang="ts">
  import '../../app.css';
  import { onMount } from 'svelte';
  import MiniPlayer from '$lib/components/MiniPlayer.svelte';
  import {
    getSettings,
    getThemes,
    onSettingsChanged,
    onThemesChanged,
    getTimerState,
    onTimerTick,
    onTimerPaused,
    onTimerResumed,
    onRoundChange,
    onTimerReset,
  } from '$lib/ipc';
  import { settings } from '$lib/stores/settings';
  import { timerState } from '$lib/stores/timer';
  import { applyTheme } from '$lib/stores/theme';
  import { setLocale } from '$lib/locale.svelte.js';
  import { resolveThemeName } from '$lib/utils/theme';
  import { isTauri } from '$lib/utils/platform';
  import { getCurrentWebviewWindow, WebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { LogicalPosition } from '@tauri-apps/api/dpi';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import type { TimerMode } from '$lib/types';

  async function saveMiniPosition() {
    if (!isTauri) return;
    try {
      const win = getCurrentWebviewWindow();
      const pos = await win.outerPosition();
      const factor = await win.scaleFactor();
      const miniPos = { x: Math.round(pos.x / factor), y: Math.round(pos.y / factor) };
      localStorage.setItem('pomotroid_mini_pos', JSON.stringify(miniPos));
    } catch {}
  }

  async function handleExpand() {
    if (!isTauri) return;
    try {
      await saveMiniPosition();
      const main = await WebviewWindow.getByLabel('main');
      if (main) {
        await main.show();
        await main.unminimize();
        await main.setFocus();
      }
      await getCurrentWebviewWindow().close();
    } catch (e) {
      console.error('Failed to expand mini player:', e);
    }
  }

  async function handleClose() {
    if (!isTauri) return;
    try {
      await saveMiniPosition();
      const main = await WebviewWindow.getByLabel('main');
      if (main) {
        await main.show();
        await main.unminimize();
      }
      await getCurrentWebviewWindow().close();
    } catch (e) {
      console.error('Failed to close mini player:', e);
    }
  }

  onMount(() => {
    const cleanups: UnlistenFn[] = [];

    (async () => {
      try {
        const s = await getSettings();
        settings.set(s);
        setLocale(s.language);

        const initialTimer = await getTimerState();
        timerState.set(initialTimer);

        const themes = await getThemes();
        const osDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
        const activeTheme = themes.find((t) => t.name === resolveThemeName(s, osDark)) ?? themes[0];
        if (activeTheme) applyTheme(activeTheme);

        cleanups.push(
          await onTimerTick(({ elapsed_secs, total_secs }) => {
            timerState.update((st) => ({
              ...st,
              elapsed_secs,
              total_secs,
              is_running: true,
              is_paused: false,
            }));
          }),
          await onTimerPaused(({ elapsed_secs }) => {
            timerState.update((st) => ({
              ...st,
              elapsed_secs,
              is_running: false,
              is_paused: true,
            }));
          }),
          await onTimerResumed(({ elapsed_secs }) => {
            timerState.update((st) => ({
              ...st,
              elapsed_secs,
              is_running: true,
              is_paused: false,
            }));
          }),
          await onRoundChange((nextSnap) => {
            timerState.set(nextSnap);
          }),
          await onTimerReset((resetSnap) => {
            timerState.set(resetSnap);
          }),
          await onSettingsChanged((updated) => {
            settings.set(updated);
            const dark = window.matchMedia('(prefers-color-scheme: dark)').matches;
            const t = themes.find((th) => th.name === resolveThemeName(updated, dark)) ?? themes[0];
            if (t) applyTheme(t);
          }),
          await onThemesChanged((updated) => {
            const dark = window.matchMedia('(prefers-color-scheme: dark)').matches;
            const t = updated.find((th) => th.name === resolveThemeName($settings, dark)) ?? updated[0];
            if (t) applyTheme(t);
          })
        );

        if (isTauri) {
          const win = getCurrentWebviewWindow();
          const savedMini = localStorage.getItem('pomotroid_mini_pos');
          if (savedMini) {
            try {
              const parsed = JSON.parse(savedMini);
              if (typeof parsed.x === 'number' && typeof parsed.y === 'number') {
                await win.setPosition(new LogicalPosition(parsed.x, parsed.y));
              }
            } catch {}
          }

          cleanups.push(
            await win.onMoved(() => {
              saveMiniPosition().catch(() => {});
            })
          );

          await win.show();
        }
      } catch (err) {
        console.error('Failed to initialize mini player window:', err);
      }
    })();

    return () => {
      cleanups.forEach((fn) => fn());
    };
  });
</script>

<div class="mini-container">
  <MiniPlayer
    mode={$settings.timer_mode as TimerMode}
    onExpand={handleExpand}
    onClose={handleClose}
  />
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background-color: var(--color-background);
  }

  .mini-container {
    width: 100vw;
    height: 100vh;
    box-sizing: border-box;
    overflow: hidden;
    border: 1px solid rgba(255, 255, 255, 0.12);
    background-color: var(--color-background);
  }
</style>
