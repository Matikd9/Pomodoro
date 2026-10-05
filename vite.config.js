import { sveltekit } from '@sveltejs/kit/vite';

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
/** @param {{ command: string }} env */
export default async ({ command }) => {
  const plugins = [];

  // In development, watch and recompile inlang messages on the fly.
  // In production build, messages are precompiled by `npm run paraglide:compile`.
  if (command === 'serve') {
    try {
      const { paraglideVitePlugin } = await import('@inlang' + '/paraglide-js');
      plugins.push(
        paraglideVitePlugin({
          project: './project.inlang',
          outdir: './src/paraglide',
          strategy: ['globalVariable', 'baseLocale'],
          emitTsDeclarations: true,
        })
      );
    } catch (e) {
      console.warn('Paraglide vite plugin not loaded in dev:', e);
    }
  }

  plugins.push(sveltekit());

  return {
    plugins,
    build: {
      cssMinify: false,
    },
    clearScreen: false,
    server: {
      port: 1420,
      strictPort: true,
      host: host || false,
      hmr: host
        ? {
            protocol: 'ws',
            host,
            port: 1421,
          }
        : undefined,
      watch: {
        ignored: ['**/src-tauri/**'],
      },
    },
  };
};
