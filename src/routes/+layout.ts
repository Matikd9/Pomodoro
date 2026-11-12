// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
import { initRemoteMode } from '$lib/ipc';

export const ssr = false;

// Decide remote vs local backend before any window renders, so an unreachable
// remote server never blocks the desktop app.
export async function load() {
  await initRemoteMode();
}
