import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

interface StartupStatus {
  message: string;
  fatal: boolean;
}

const message = document.getElementById('startup-message');
const help = document.getElementById('startup-help');
const actions = document.getElementById('startup-actions');
const exit = document.getElementById('startup-exit');
const screen = document.getElementById('startup-screen');

function showFatal(errorMessage: string) {
  if (screen) {
    screen.dataset.state = 'error';
    screen.setAttribute('role', 'alert');
    screen.setAttribute('aria-label', 'Skycomet startup failed');
  }
  if (message) message.textContent = 'Skycomet could not start';
  if (help) {
    help.textContent = errorMessage;
    help.hidden = false;
  }
  if (actions) actions.hidden = false;
}

async function start() {
  await listen<StartupStatus>('startup_status', ({ payload }) => {
    if (payload.fatal) {
      showFatal(payload.message);
    } else if (message) {
      message.textContent = payload.message;
    }
  });

  // Let the lightweight splash reach the compositor before database and
  // catalog initialization starts on a blocking worker.
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      void invoke('begin_startup').catch((error: unknown) => {
        showFatal(`Initialization could not begin: ${String(error)}`);
      });
    });
  });
}

exit?.addEventListener('click', () => {
  void invoke('abort_startup');
});

void start().catch((error: unknown) => {
  showFatal(`The startup screen could not initialize: ${String(error)}`);
});
