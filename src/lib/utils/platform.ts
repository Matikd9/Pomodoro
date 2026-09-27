export const isTauri =
  typeof window !== 'undefined' &&
  ('__TAURI_INTERNALS__' in window ||
    '__TAURI__' in window ||
    window.location.hostname === 'tauri.localhost' ||
    window.location.protocol === 'tauri:');

/** True when running on macOS inside the Tauri desktop app. */
export const isMac = isTauri && /Macintosh|Mac OS X/.test(navigator.userAgent);

/** True when running on Linux inside the Tauri desktop app. */
export const isLinux = isTauri && /Linux/.test(navigator.userAgent);

/** True when running on mobile or small touch screen. */
export const isMobile =
  typeof window !== 'undefined' &&
  (/Android|iPhone|iPad|iPod|webOS|BlackBerry|IEMobile|Opera Mini/i.test(navigator.userAgent) ||
    window.innerWidth < 600);

/** Opens an external URL safely in both Tauri desktop and standard web browsers. */
export async function openExternalUrl(url: string): Promise<void> {
  if (isTauri) {
    try {
      const { openUrl } = await import('@tauri-apps/plugin-opener');
      await openUrl(url);
      return;
    } catch {
      // Fall through to window.open
    }
  }
  if (typeof window !== 'undefined') {
    window.open(url, '_blank');
  }
}
