<script lang="ts">
  import { settings } from '$lib/stores/settings';
  import { setSetting, telegramTest } from '$lib/ipc';
  import SettingsToggle from '$lib/components/settings/SettingsToggle.svelte';
  import * as m from '$paraglide/messages.js';

  let botToken = $state($settings.telegram_bot_token);
  let chatId = $state($settings.telegram_chat_id);

  $effect(() => {
    botToken = $settings.telegram_bot_token;
    chatId = $settings.telegram_chat_id;
  });

  let testStatus = $state<'idle' | 'loading' | 'success' | 'error'>('idle');
  let statusMessage = $state<string>('');

  async function toggleTelegram() {
    try {
      const nextVal = !$settings.telegram_enabled;
      settings.update((s) => ({ ...s, telegram_enabled: nextVal }));
      const updated = await setSetting('telegram_enabled', nextVal ? 'true' : 'false');
      settings.set(updated);
    } catch (err) {
      console.error('Failed to toggle telegram:', err);
      settings.update((s) => ({ ...s, telegram_enabled: !$settings.telegram_enabled }));
    }
  }

  async function handleTokenBlur() {
    try {
      if (botToken !== $settings.telegram_bot_token) {
        settings.update((s) => ({ ...s, telegram_bot_token: botToken.trim() }));
        const updated = await setSetting('telegram_bot_token', botToken.trim());
        settings.set(updated);
      }
    } catch (err) {
      console.error('Failed to save bot token:', err);
    }
  }

  async function handleChatIdBlur() {
    try {
      if (chatId !== $settings.telegram_chat_id) {
        settings.update((s) => ({ ...s, telegram_chat_id: chatId.trim() }));
        const updated = await setSetting('telegram_chat_id', chatId.trim());
        settings.set(updated);
      }
    } catch (err) {
      console.error('Failed to save chat id:', err);
    }
  }

  async function handleTest() {
    if (!botToken.trim() || !chatId.trim()) {
      testStatus = 'error';
      statusMessage = 'Por favor ingresa tanto el Bot Token como el Chat ID antes de probar.';
      return;
    }

    testStatus = 'loading';
    statusMessage = m.telegram_test_testing();

    try {
      // First ensure current input values are saved
      await setSetting('telegram_bot_token', botToken.trim());
      await setSetting('telegram_chat_id', chatId.trim());

      const res = await telegramTest(botToken.trim(), chatId.trim());
      testStatus = 'success';
      statusMessage = res || m.telegram_test_success();
      setTimeout(() => {
        if (testStatus === 'success') {
          testStatus = 'idle';
          statusMessage = '';
        }
      }, 5000);
    } catch (err) {
      testStatus = 'error';
      statusMessage = String(err);
    }
  }
</script>

<div class="section">
  <span class="group-title">{m.telegram_group_title()}</span>

  <!-- Enable Toggle -->
  <SettingsToggle
    checked={$settings.telegram_enabled}
    label={m.telegram_toggle()}
    description={m.telegram_toggle_desc()}
    onclick={toggleTelegram}
  />

  <div class="fields-container" class:disabled={!$settings.telegram_enabled}>
    <!-- Bot Token Field -->
    <div class="field-row">
      <div class="field-meta">
        <label for="bot-token" class="field-label">{m.telegram_token_label()}</label>
      </div>
      <input
        id="bot-token"
        type="password"
        class="input-text"
        placeholder={m.telegram_token_placeholder()}
        bind:value={botToken}
        onblur={handleTokenBlur}
        disabled={!$settings.telegram_enabled}
        spellcheck="false"
        autocomplete="off"
      />
    </div>

    <!-- Chat ID Field -->
    <div class="field-row">
      <div class="field-meta">
        <label for="chat-id" class="field-label">{m.telegram_chat_id_label()}</label>
      </div>
      <input
        id="chat-id"
        type="text"
        class="input-text"
        placeholder={m.telegram_chat_id_placeholder()}
        bind:value={chatId}
        onblur={handleChatIdBlur}
        disabled={!$settings.telegram_enabled}
        spellcheck="false"
        autocomplete="off"
      />
    </div>

    <!-- Test Button & Status Row -->
    <div class="action-row">
      <button
        class="btn-test"
        onclick={handleTest}
        disabled={!$settings.telegram_enabled || testStatus === 'loading'}
      >
        {#if testStatus === 'loading'}
          <svg class="spinner" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M21 12a9 9 0 1 1-6.219-8.56" />
          </svg>
          <span>{m.telegram_test_testing()}</span>
        {:else}
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="22" y1="2" x2="11" y2="13" />
            <polygon points="22 2 15 22 11 13 2 9 22 2" />
          </svg>
          <span>{m.telegram_test_btn()}</span>
        {/if}
      </button>
    </div>

    {#if statusMessage}
      <div class="status-banner" class:success={testStatus === 'success'} class:error={testStatus === 'error'}>
        {#if testStatus === 'success'}
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="20 6 9 17 4 12" />
          </svg>
        {:else if testStatus === 'error'}
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="12" cy="12" r="10" />
            <line x1="12" y1="8" x2="12" y2="12" />
            <line x1="12" y1="16" x2="12.01" y2="16" />
          </svg>
        {/if}
        <span>{statusMessage}</span>
      </div>
    {/if}
  </div>

  <!-- Help Guide -->
  <div class="guide-box">
    <div class="guide-header">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="12" cy="12" r="10" />
        <line x1="12" y1="16" x2="12" y2="12" />
        <line x1="12" y1="8" x2="12.01" y2="8" />
      </svg>
      <span class="guide-title">{m.telegram_help_title()}</span>
    </div>
    <ol class="guide-steps">
      <li>{m.telegram_help_step1()}</li>
      <li>{m.telegram_help_step2()}</li>
      <li>{m.telegram_help_step3()}</li>
    </ol>
  </div>
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    padding-bottom: 24px;
  }

  .group-title {
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-foreground-darker, rgba(255, 255, 255, 0.4));
    padding: 12px 16px 4px;
  }

  .fields-container {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 8px 16px;
    transition: opacity var(--transition-snappy);
  }

  .fields-container.disabled {
    opacity: 0.45;
    pointer-events: none;
  }

  .field-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .field-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .field-label {
    font-size: 0.76rem;
    font-weight: 600;
    color: var(--color-foreground);
  }

  .input-text {
    width: 100%;
    box-sizing: border-box;
    background: var(--color-background-light, rgba(255, 255, 255, 0.05));
    border: 1px solid color-mix(in oklch, var(--color-foreground) 15%, transparent);
    border-radius: 6px;
    padding: 8px 10px;
    font-size: 0.78rem;
    color: var(--color-foreground);
    outline: none;
    transition: border-color var(--transition-snappy);
    font-family: inherit;
  }

  .input-text:focus {
    border-color: var(--color-focus-round);
  }

  .action-row {
    display: flex;
    justify-content: flex-end;
    margin-top: 4px;
  }

  .btn-test {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 14px;
    border-radius: 6px;
    background: color-mix(in oklch, var(--color-foreground) 8%, transparent);
    border: 1px solid color-mix(in oklch, var(--color-foreground) 15%, transparent);
    color: var(--color-foreground);
    font-size: 0.74rem;
    font-weight: 600;
    cursor: pointer;
    transition: all var(--transition-snappy);
  }

  .btn-test:hover:not(:disabled) {
    background: color-mix(in oklch, var(--color-focus-round) 15%, transparent);
    border-color: var(--color-focus-round);
    color: var(--color-focus-round);
  }

  .btn-test:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .status-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-radius: 6px;
    font-size: 0.74rem;
    margin-top: 4px;
  }

  .status-banner.success {
    background: color-mix(in oklch, #4caf50 15%, transparent);
    border: 1px solid color-mix(in oklch, #4caf50 30%, transparent);
    color: #81c784;
  }

  .status-banner.error {
    background: color-mix(in oklch, #e05252 15%, transparent);
    border: 1px solid color-mix(in oklch, #e05252 30%, transparent);
    color: #e57373;
  }

  .guide-box {
    margin: 16px 16px 0;
    padding: 12px 14px;
    border-radius: 8px;
    background: color-mix(in oklch, var(--color-foreground) 4%, transparent);
    border: 1px solid color-mix(in oklch, var(--color-foreground) 10%, transparent);
  }

  .guide-header {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--color-foreground-darker);
    margin-bottom: 8px;
  }

  .guide-title {
    font-size: 0.74rem;
    font-weight: 600;
  }

  .guide-steps {
    margin: 0;
    padding-left: 18px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 0.7rem;
    color: var(--color-foreground-darker);
    line-height: 1.4;
  }

  .spinner {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }
</style>
