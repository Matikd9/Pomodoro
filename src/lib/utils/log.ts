import { isTauri } from '$lib/utils/platform';

export async function logInfo(message: string): Promise<void> {
  if (isTauri) {
    try {
      const { info } = await import('@tauri-apps/plugin-log');
      await info(message);
      return;
    } catch {
      // Fall through to console
    }
  }
  console.log(message);
}

export async function logError(message: string): Promise<void> {
  if (isTauri) {
    try {
      const { error } = await import('@tauri-apps/plugin-log');
      await error(message);
      return;
    } catch {
      // Fall through to console
    }
  }
  console.error(message);
}

export async function logWarn(message: string): Promise<void> {
  if (isTauri) {
    try {
      const { warn } = await import('@tauri-apps/plugin-log');
      await warn(message);
      return;
    } catch {
      // Fall through to console
    }
  }
  console.warn(message);
}
