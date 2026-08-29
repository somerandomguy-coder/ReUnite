/* ════════════════════════════════════════════════════════════
   ReUnite — runtime configuration

   SECURITY NOTE
   A Google Maps JS API key is always visible to anyone who opens
   this page — unavoidable for a browser-side map. The only real
   protection is a restriction in Google Cloud Console:

     APIs & Services → Credentials → (this key) → Restrictions
       • Application restrictions: HTTP referrers
           https://your-domain.com/*
           http://localhost:4321/*
       • API restrictions: Maps JavaScript API only

   The key below has been shared in plain text, so rotate it in the
   console and paste the replacement here before going live.
   ════════════════════════════════════════════════════════════ */
window.ReUniteConfig = {
  googleMapsKey: 'YOUR_GOOGLE_MAPS_API_KEY'
};

/* Small ready-gate so several modules can share one async Maps load. */
window.ReUniteMaps = {
  ready: false,
  failed: false,
  _cbs: [],
  onReady: function (fn) { this.ready ? fn() : this._cbs.push(fn); },
  _fire: function () { this.ready = true; this._cbs.splice(0).forEach(function (f) { f(); }); }
};

window.__reuniteGmapsInit = function () { window.ReUniteMaps._fire(); };

/* Google calls this when the key is rejected — bad key, blocked referrer,
   or billing not enabled on the project. Fall back rather than show a
   broken map. */
window.gm_authFailure = function () {
  window.ReUniteMaps.failed = true;
  console.error('[ReUnite] Google Maps rejected the API key — check billing and referrer restrictions.');
  document.dispatchEvent(new CustomEvent('reunite:maps-failed'));
};

/* Inject the loader once the config above exists. */
(function () {
  var k = window.ReUniteConfig.googleMapsKey;
  var s = document.createElement('script');
  s.src = 'https://maps.googleapis.com/maps/api/js?key=' + encodeURIComponent(k) +
          '&v=weekly&loading=async&callback=__reuniteGmapsInit';
  s.async = true;
  s.onerror = function () { window.gm_authFailure(); };
  document.head.appendChild(s);
})();
