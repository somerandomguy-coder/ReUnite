/* ════════════════════════════════════════════════════════════
   ReUnite — pulsing beacon rings under the victim pins

   A Google Maps OverlayView that positions one small element per
   signal at its ground coordinate. The rings themselves are pure
   CSS animation, so nothing runs per frame in JS — draw() only
   moves the anchors when the map does.
   ════════════════════════════════════════════════════════════ */
window.ReUnitePulseLayer = function (opts) {
  'use strict';

  const signals = opts.signals || [];
  const colorOf = opts.colorOf || function () { return '#E11D48'; };
  const REDUCED = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  const Layer = function () {};
  Layer.prototype = new google.maps.OverlayView();

  Layer.prototype.onAdd = function () {
    const wrap = document.createElement('div');
    wrap.className = 'pulses';
    if (REDUCED) wrap.classList.add('is-still');

    this.nodes = {};
    const self = this;
    signals.forEach(function (s) {
      const el = document.createElement('div');
      el.className = 'pulse';
      el.style.setProperty('--c', colorOf());
      /* three rings on staggered delays read as a repeating beacon */
      el.innerHTML = '<i></i><i></i><i></i>';
      wrap.appendChild(el);
      self.nodes[s.id] = { el: el, signal: s, on: true };
    });

    this.wrap = wrap;
    this.getPanes().overlayLayer.appendChild(wrap);
  };

  Layer.prototype.onRemove = function () {
    if (this.wrap && this.wrap.parentNode) this.wrap.parentNode.removeChild(this.wrap);
    this.wrap = null;
    this.nodes = {};
  };

  Layer.prototype.draw = function () {
    const proj = this.getProjection();
    if (!proj || !this.nodes) return;
    const self = this;
    Object.keys(this.nodes).forEach(function (id) {
      const n = self.nodes[id];
      const p = proj.fromLatLngToDivPixel(
        new google.maps.LatLng(n.signal.lat, n.signal.lon)
      );
      n.el.style.transform = 'translate(' + p.x + 'px,' + p.y + 'px)';
    });
  };

  /* follow the same layer toggle as the pins */
  Layer.prototype.apply = function (layerOn) {
    if (!this.nodes) return;
    const self = this;
    Object.keys(this.nodes).forEach(function (id) {
      self.nodes[id].el.style.display = layerOn ? '' : 'none';
    });
  };

  /* the selected signal rings harder than the rest */
  Layer.prototype.select = function (id) {
    if (!this.nodes) return;
    const self = this;
    Object.keys(this.nodes).forEach(function (k) {
      self.nodes[k].el.classList.toggle('is-sel', k === id);
    });
  };

  return new Layer();
};
