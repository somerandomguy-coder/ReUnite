/* ════════════════════════════════════════════════════════════
   ReUnite — theme flag and toggle

   Light is the default. One attribute on <html> flips everything:
   the CSS tokens, the Google Maps style, the heat ramp, the globe,
   and the priority palette the markers read.

     <html lang="en" data-theme="dark">
     ReUniteTheme.set('dark')  ·  ReUniteTheme.toggle()

   The choice is kept in localStorage. The inline script in each
   page's <head> applies it before first paint, so the wrong theme
   never flashes.
   ════════════════════════════════════════════════════════════ */
window.ReUniteTheme = (function () {
  'use strict';

  const root = document.documentElement;
  const KEY = 'reunite-theme';

  function current() {
    return root.getAttribute('data-theme') === 'dark' ? 'dark' : 'light';
  }

  function store(name) {
    try { localStorage.setItem(KEY, name); } catch (e) { /* private mode */ }
  }

  const api = {
    get name() { return current(); },
    get isDark() { return current() === 'dark'; },

    /* pick(lightValue, darkValue) */
    pick: function (light, dark) { return current() === 'dark' ? dark : light; },

    /* read a themed CSS custom property, so JS surfaces stay in step
       with the stylesheet instead of duplicating hex values */
    css: function (name, fallback) {
      const v = getComputedStyle(root).getPropertyValue(name).trim();
      return v || fallback;
    },

    set: function (name) {
      const next = name === 'dark' ? 'dark' : 'light';
      if (next === 'dark') root.setAttribute('data-theme', 'dark');
      else root.removeAttribute('data-theme');
      store(next);
      syncButtons();
      document.dispatchEvent(new CustomEvent('reunite:theme', { detail: { name: next } }));
      return next;
    },

    toggle: function () { return api.set(current() === 'dark' ? 'light' : 'dark'); }
  };

  /* ── toggle buttons ──────────────────────────────────── */
  function syncButtons() {
    const dark = current() === 'dark';
    document.querySelectorAll('[data-theme-toggle]').forEach(function (b) {
      b.setAttribute('aria-pressed', String(dark));
      b.setAttribute('aria-label', dark ? 'Switch to light theme' : 'Switch to dark theme');
      b.setAttribute('title', dark ? 'Switch to light theme' : 'Switch to dark theme');
    });
  }

  function wire() {
    document.querySelectorAll('[data-theme-toggle]').forEach(function (b) {
      if (b.dataset.wired) return;
      b.dataset.wired = '1';
      b.addEventListener('click', function () { api.toggle(); });
    });
    syncButtons();
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', wire);
  } else {
    wire();
  }

  return api;
})();
