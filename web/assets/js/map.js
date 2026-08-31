/* ════════════════════════════════════════════════════════════
   ReUnite — live SOS heatmap (Google Maps JavaScript API)
   Classic google.maps.Marker is used rather than AdvancedMarker
   because the JSON `styles` palette above needs a map without a
   cloud Map ID, and the two are mutually exclusive.
   ════════════════════════════════════════════════════════════ */
(function () {
  'use strict';

  const D = window.ReUniteData;
  const host = document.getElementById('mapCanvas');
  const failBox = document.getElementById('mapFail');
  const listEl = document.getElementById('sigList');
  const countEl = document.getElementById('sigCount');
  if (!host || !D) return;

  const REDUCED = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  /* the dark set is brightened — the light values sink into night tiles */
  /* Every SOS signal is the same red — the map answers "where are the
     people who need help", not "what kind of help". Colours come from the
     stylesheet so a theme switch only has to change CSS. */
  let DARK = false, SOS = '#E11D48', STROKE = '#ffffff', AIDPOST = '#047857';

  function readPalette() {
    const T = window.ReUniteTheme;
    DARK = !!(T && T.isDark);
    const v = T ? function (n, f) { return T.css(n, f); } : function (n, f) { return f; };
    SOS     = v('--sos-solid', '#E11D48');
    AIDPOST = v('--safe', '#047857');
    STROKE  = DARK ? '#0F1115' : '#ffffff';
  }
  readPalette();

  const esc = function (s) {
    return String(s).replace(/[&<>"']/g, function (c) {
      return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c];
    });
  };
  const ago = function (m) { return m < 60 ? m + 'm ago' : Math.round(m / 60) + 'h ago'; };

  /* ── signal log — also the accessible equivalent of the map ── */
  let selected = null;

  function renderLog() {
    const rows = D.signals;
    countEl.textContent = rows.length;
    listEl.innerHTML = '';

    rows.forEach(function (s) {
      const li = document.createElement('li');
      const b = document.createElement('button');
      b.className = 'sig' + (selected === s.id ? ' is-sel' : '');
      b.type = 'button';
      b.dataset.id = s.id;
      b.setAttribute('aria-label',
        'SOS from ' + s.who + ', ' + s.hops + ' hops, TTL ' + s.ttl +
        ', battery ' + s.batt + ' percent, ' + ago(s.age) + '. ' + s.msg);
      b.innerHTML =
        '<span class="sig__top">' +
          '<span class="sig__prio">SOS</span>' +
          '<span class="sig__who">' + esc(s.who) + '</span>' +
          '<span class="sig__age">' + ago(s.age) + '</span>' +
        '</span>' +
        '<span class="sig__msg">' + esc(s.msg) + '</span>' +
        '<span class="sig__meta">' +
          '<span>HOPS <b>' + s.hops + '/8</b></span>' +
          '<span>TTL <b>' + s.ttl + '</b></span>' +
          '<span>BATT <b>' + s.batt + '%</b></span>' +
          '<span>' + s.lat.toFixed(4) + ', ' + s.lon.toFixed(4) + '</span>' +
        '</span>';
      b.addEventListener('click', function () { focusSignal(s.id, 'list'); });
      li.appendChild(b);
      listEl.appendChild(li);
    });
  }

  /* ── state ───────────────────────────────────────────── */
  let map = null, heat = null, safeHeat = null, info = null, pulses = null;
  const sosMarkers = {};                       // id -> { dot, signal }
  const markers = { aid: [] };
  const shown = { safe: true, heat: true, points: true, aid: true };

  function bounds() {
    const b = new google.maps.LatLngBounds();
    D.heat.features.forEach(function (f) {
      b.extend({ lat: f.geometry.coordinates[1], lng: f.geometry.coordinates[0] });
    });
    D.signals.forEach(function (s) { b.extend({ lat: s.lat, lng: s.lon }); });
    return b;
  }

  /* Overlays (the floating control card, the legend) sit on top of the map,
     so the fit has to leave room for them or the densest clusters end up
     hidden underneath. */
  function fitPadding() {
    const h = host.clientHeight || 520;
    const tools = document.querySelector('.apptools');
    const legend = document.querySelector('.mapapp .maplegend');
    const top = tools ? Math.min(tools.offsetTop + tools.offsetHeight + 18, h * 0.42) : 46;
    const bottom = legend ? Math.min(legend.offsetHeight + 34, h * 0.28) : 54;
    return { top: Math.round(top), right: 26, bottom: Math.round(bottom), left: 26 };
  }

  function home(animate) {
    if (!map) return;
    if (animate && !REDUCED) map.panTo(bounds().getCenter());
    map.fitBounds(bounds(), fitPadding());
  }

  /* Victim locations are teardrop pins anchored on the exact coordinate,
     with the SOS red in the body and a white core. Built as an SVG
     data URI because a google.maps.Symbol path can only carry one fill. */
  function pinIcon(color, big) {
    const w = big ? 38 : 30, h = big ? 53 : 42;
    const svg =
      '<svg xmlns="http://www.w3.org/2000/svg" width="30" height="42" viewBox="0 0 30 42">' +
        '<path d="M15 40.5C15 40.5 27.5 24.2 27.5 14.8A12.5 12.5 0 1 0 2.5 14.8C2.5 24.2 15 40.5 15 40.5Z" ' +
              'fill="' + color + '" stroke="#ffffff" stroke-width="2.4" stroke-linejoin="round"/>' +
        '<circle cx="15" cy="14.8" r="4.6" fill="#ffffff"/>' +
      '</svg>';
    return {
      url: 'data:image/svg+xml;charset=UTF-8,' + encodeURIComponent(svg),
      scaledSize: new google.maps.Size(w, h),
      anchor: new google.maps.Point(w / 2, h)
    };
  }

  function popupHTML(title, color, body, meta) {
    return '<div class="pop">' +
      '<div class="pop__h"><span class="dot" style="background:' + color + '"></span>' +
      '<span class="pop__t">' + esc(title) + '</span></div>' +
      '<p class="pop__p">' + esc(body) + '</p>' +
      '<p class="pop__m">' + esc(meta) + '</p></div>';
  }

  function openInfo(marker, html) {
    if (!info) info = new google.maps.InfoWindow();
    info.setContent(html);
    info.open({ map: map, anchor: marker });
  }

  function focusSignal(id, source) {
    const s = D.signals.find(function (x) { return x.id === id; });
    if (!s) return;
    selected = id;
    Array.prototype.forEach.call(listEl.querySelectorAll('.sig'), function (el) {
      el.classList.toggle('is-sel', el.dataset.id === id);
    });
    document.dispatchEvent(new CustomEvent('reunite:signal', {
      detail: { id: id, source: source || 'map' }
    }));
    const entry = sosMarkers[id];
    if (!map || !entry) return;
    Object.keys(sosMarkers).forEach(function (k) {
      const e = sosMarkers[k];
      e.dot.setIcon(pinIcon(SOS, k === id));
      e.dot.setZIndex(k === id ? 40 : e.dot.getZIndex());
    });
    if (pulses) pulses.select(id);
    map.panTo({ lat: s.lat, lng: s.lon });
    map.setZoom(14);
    openInfo(entry.dot, popupHTML(
      'SOS · ' + s.id, SOS, s.msg,
      'HOPS ' + s.hops + '/8 · TTL ' + s.ttl + ' · BATT ' + s.batt + '% · ' +
      (s.needs.length ? 'NEEDS ' + s.needs.join(', ').toUpperCase() : 'NO REQUEST')));
  }

  /* ── build ───────────────────────────────────────────── */
  function build() {
    map = new google.maps.Map(host, {
      styles: window.ReUniteMapStyle || [],
      mapTypeControl: false,
      streetViewControl: false,
      fullscreenControl: false,
      rotateControl: false,
      clickableIcons: false,
      isFractionalZoomEnabled: true,
      cameraControl: false,
      gestureHandling: 'cooperative',
      zoomControl: true,
      zoomControlOptions: { position: google.maps.ControlPosition.RIGHT_TOP },
      center: { lat: D.center[1], lng: D.center[0] },
      zoom: 10
    });
    window.__reuniteMap = map;
    /* the card is still laying out at construction time, so re-fit once
       the map reports its real size */
    home(false);
    google.maps.event.addListenerOnce(map, 'idle', function () { home(false); });

    /* distress density — custom canvas overlay, since Google
       discontinued google.maps.visualization.HeatmapLayer */
    makeHeat();

    /* victim locations: teardrop pins, one red for every signal */
    D.signals.forEach(function (s) {
      const pos = { lat: s.lat, lng: s.lon };

      const dot = new google.maps.Marker({
        position: pos, map: map, optimized: false,
        zIndex: 21,
        title: 'SOS · ' + s.id,
        icon: pinIcon(SOS, false)
      });
      dot.addListener('click', function () { focusSignal(s.id, 'map'); });
      sosMarkers[s.id] = { dot: dot, signal: s };
    });

    /* aid posts sit as small squares, well under the SOS pins */
    D.aid.features.forEach(function (f) {
      const p = f.properties, c = f.geometry.coordinates;
      const m = new google.maps.Marker({
        position: { lat: c[1], lng: c[0] }, map: map, zIndex: 12,
        title: 'Aid post: ' + p.kind + '. ' + p.note,
        icon: { path: 'M -7.5,-7.5 L 7.5,-7.5 L 7.5,7.5 L -7.5,7.5 Z', scale: 1,
                fillColor: AIDPOST, fillOpacity: 1, strokeColor: STROKE, strokeWeight: 2.5 }
      });
      m.addListener('click', function () {
        openInfo(m, popupHTML(p.kind, AIDPOST, p.note, 'AID POST · RELAYED BY MESH'));
      });
      markers.aid.push(m);
    });

    applyVisibility();

    /* pulsing beacon rings, sitting under the pins at ground level */
    pulses = window.ReUnitePulseLayer({
      signals: D.signals,
      colorOf: function () { return 'var(--sos-solid)'; }
    });
    pulses.setMap(map);

    /* The map is inside a flex column (map.html) and a revealed-on-scroll
       card (index.html), so at construction the container can still be 0px
       tall — the first fit lands on garbage. Watch both dimensions and
       re-fit whenever either moves. */
    if ('ResizeObserver' in window) {
      let lastW = 0, lastH = 0, t;
      new ResizeObserver(function () {
        clearTimeout(t);
        t = setTimeout(function () {
          const w = host.clientWidth, h = host.clientHeight;
          if (!w || !h) return;
          if (Math.abs(w - lastW) < 40 && Math.abs(h - lastH) < 40) return;
          lastW = w; lastH = h;
          home(false);
        }, 160);
      }).observe(host);
    }

    /* belt and braces for browsers without ResizeObserver */
    requestAnimationFrame(function () { requestAnimationFrame(function () { home(false); }); });
  }

  function makeHeat() {
    /* The safe field is added first so it sits under the distress field —
       where the two overlap, distress has to win. */
    if (safeHeat) safeHeat.setMap(null);
    safeHeat = window.ReUniteHeatLayer({
      ramp: 'safe',
      peakAlpha: 0.11,
      points: D.safe.features.map(function (f) {
        return {
          lat: f.geometry.coordinates[1],
          lng: f.geometry.coordinates[0],
          weight: f.properties.severity
        };
      })
    });
    safeHeat.setMap(map);
    safeHeat.setVisible(shown.safe);

    if (heat) heat.setMap(null);
    heat = window.ReUniteHeatLayer({
      ramp: 'distress',
      peakAlpha: 0.11,
      points: D.heat.features.map(function (f) {
        return {
          lat: f.geometry.coordinates[1],
          lng: f.geometry.coordinates[0],
          weight: f.properties.severity
        };
      })
    });
    heat.setMap(map);
    heat.setVisible(shown.heat);
  }

  /* repaint every themed surface when the toggle flips */
  document.addEventListener('reunite:theme', function () {
    readPalette();
    if (!map) return;
    map.setOptions({
      styles: window.ReUniteTheme.pick(window.ReUniteMapStyleLight, window.ReUniteMapStyleDark)
    });
    Object.keys(sosMarkers).forEach(function (id) {
      const e = sosMarkers[id];
      e.dot.setIcon(pinIcon(SOS, id === selected));
    });
    markers.aid.forEach(function (m) {
      const ic = m.getIcon(); ic.fillColor = AIDPOST; ic.strokeColor = STROKE; m.setIcon(ic);
    });
    makeHeat();
  });

  function applyVisibility() {
    if (heat) heat.setVisible(shown.heat);
    if (safeHeat) safeHeat.setVisible(shown.safe);
    Object.keys(sosMarkers).forEach(function (id) {
      const e = sosMarkers[id];
      e.dot.setVisible(shown.points);
    });
    markers.aid.forEach(function (m) { m.setVisible(shown.aid); });
    if (pulses) pulses.apply(shown.points);
  }

  /* ── controls ────────────────────────────────────────── */
  document.querySelectorAll('[data-layer]').forEach(function (btn) {
    btn.addEventListener('click', function () {
      const on = !btn.classList.contains('is-on');
      btn.classList.toggle('is-on', on);
      btn.setAttribute('aria-pressed', String(on));
      shown[btn.dataset.layer] = on;
      applyVisibility();
    });
  });

  const resetBtn = document.getElementById('mapReset');
  if (resetBtn) {
    resetBtn.addEventListener('click', function () {
      selected = null;
      if (info) info.close();
      if (pulses) pulses.select(null);
      home(true);
    });
  }

  /* ── boot ────────────────────────────────────────────── */
  renderLog();

  document.addEventListener('reunite:maps-failed', function () { failBox.hidden = false; });

  const failTimer = setTimeout(function () {
    if (!map) failBox.hidden = false;
  }, 12000);

  /* config.js is not committed — it carries the API key — so a fresh clone
     has no loader at all. Fail into the readable fallback rather than
     throwing. */
  if (!window.ReUniteMaps) {
    clearTimeout(failTimer);
    failBox.hidden = false;
    console.error('[ReUnite] assets/js/config.js is missing. Copy config.example.js to config.js and add your Google Maps API key.');
    return;
  }

  window.ReUniteMaps.onReady(function () {
    clearTimeout(failTimer);
    failBox.hidden = true;
    try {
      build();
    } catch (e) {
      failBox.hidden = false;
      console.error('[ReUnite] map build failed', e);
    }
  });
})();
