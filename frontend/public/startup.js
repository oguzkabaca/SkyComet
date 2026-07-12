(function () {
  'use strict';

  var STORAGE_KEY = 'skycomet.theme';
  var THEMES = ['calm', 'paper', 'fog', 'dark', 'midnight', 'console'];
  var READY_EVENT = 'skycomet:ready';
  var FAILURE_DELAY_MS = 15000;
  var theme = 'calm';

  try {
    var stored = localStorage.getItem(STORAGE_KEY);
    if (THEMES.indexOf(stored) !== -1) theme = stored;
  } catch (_error) {
    // Storage can be unavailable without making startup fatal. Calm remains
    // the deterministic fallback shared with ThemeProvider.
  }

  document.documentElement.dataset.theme = theme;
  performance.mark('skycomet:startup-script');

  var failureTimer = window.setTimeout(function () {
    var screen = document.getElementById('startup-screen');
    var message = document.getElementById('startup-message');
    var help = document.getElementById('startup-help');
    var actions = document.getElementById('startup-actions');
    if (!screen || screen.dataset.state === 'ready') return;

    screen.dataset.state = 'error';
    screen.setAttribute('role', 'alert');
    screen.setAttribute('aria-label', 'Skycomet startup failed');
    if (message) message.textContent = 'Startup is taking longer than expected';
    if (help) help.hidden = false;
    if (actions) actions.hidden = false;
  }, FAILURE_DELAY_MS);

  window.addEventListener(
    READY_EVENT,
    function () {
      window.clearTimeout(failureTimer);
      performance.mark('skycomet:application-ready');
      var screen = document.getElementById('startup-screen');
      if (!screen) return;
      screen.dataset.state = 'ready';
      screen.setAttribute('aria-hidden', 'true');
      window.setTimeout(function () {
        screen.remove();
      }, 180);
    },
    { once: true },
  );

  function reportBootError(event) {
    window.clearTimeout(failureTimer);
    var screen = document.getElementById('startup-screen');
    var message = document.getElementById('startup-message');
    var help = document.getElementById('startup-help');
    if (!screen || screen.dataset.state === 'ready') return;
    screen.dataset.state = 'error';
    screen.setAttribute('role', 'alert');
    screen.setAttribute('aria-label', 'Skycomet startup failed');
    if (message) message.textContent = 'Skycomet could not start';
    if (help) help.hidden = false;
    console.error('Skycomet startup failed', event.error || event.reason || event);
  }

  window.addEventListener('error', reportBootError);
  window.addEventListener('unhandledrejection', reportBootError);
})();
