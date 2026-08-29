/* ════════════════════════════════════════════════════════════
   ReUnite — signal log drawer (full-screen map page only)

   The drawer overlays the map rather than resizing it, so nothing
   has to be re-fitted when it opens. map.js is untouched by this;
   it only listens for the `reunite:signal` event map.js emits.
   ════════════════════════════════════════════════════════════ */
(function () {
  'use strict';

  const drawer = document.getElementById('drawer');
  const toggle = document.getElementById('logToggle');
  const closeBtn = document.getElementById('logClose');
  const scrim = document.getElementById('drawerScrim');
  const countEl = document.getElementById('sigCount');
  const badge = document.getElementById('logCount');
  if (!drawer || !toggle) return;

  let open = false;
  let lastFocus = null;

  function setOpen(next) {
    open = next;
    drawer.classList.toggle('is-open', open);
    drawer.setAttribute('aria-hidden', String(!open));
    toggle.setAttribute('aria-expanded', String(open));
    scrim.hidden = !open;

    if (open) {
      lastFocus = document.activeElement;
      const first = drawer.querySelector('.sig') || closeBtn;
      if (first) first.focus({ preventScroll: true });
    } else if (lastFocus && document.contains(lastFocus)) {
      lastFocus.focus({ preventScroll: true });
    }
  }

  toggle.addEventListener('click', function () { setOpen(!open); });
  closeBtn.addEventListener('click', function () { setOpen(false); });
  scrim.addEventListener('click', function () { setOpen(false); });

  document.addEventListener('keydown', function (e) {
    if (e.key === 'Escape' && open) { e.preventDefault(); setOpen(false); }
  });

  /* keep the count on the toggle in step with the filtered list */
  function syncBadge() { if (badge && countEl) badge.textContent = countEl.textContent; }
  if (countEl && 'MutationObserver' in window) {
    new MutationObserver(syncBadge).observe(countEl, { childList: true, characterData: true, subtree: true });
  }
  syncBadge();

  /* picking a marker on the map should reveal that row, not just select it */
  document.addEventListener('reunite:signal', function (e) {
    if (!e.detail || e.detail.source !== 'map') return;
    if (!open) setOpen(true);
    const row = drawer.querySelector('.sig.is-sel');
    if (row) row.scrollIntoView({ block: 'nearest' });
  });

  /* choosing a row on a phone hands the screen back to the map */
  drawer.addEventListener('click', function (e) {
    if (window.innerWidth > 720) return;
    if (e.target.closest('.sig')) setOpen(false);
  });
})();
