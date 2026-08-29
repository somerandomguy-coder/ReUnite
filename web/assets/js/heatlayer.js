/* ════════════════════════════════════════════════════════════
   ReUnite — canvas heatmap overlay for Google Maps

   Google discontinued google.maps.visualization.HeatmapLayer, so
   density is rendered here instead: weighted radial gradients are
   accumulated into an alpha mask, then that mask is mapped through
   a colour ramp. Rendering happens on `idle` only — while the user
   drags, the canvas rides along with the overlay pane.
   ════════════════════════════════════════════════════════════ */
window.ReUniteHeatLayer = function (opts) {
  'use strict';

  const points = opts.points || [];          // [{ lat, lng, weight }]
  /* Named ramps. `distress` is rose, `safe` is green; each has a light
     and a dark variant so the field reads on either page. */
  const RAMPS = {
    distress: {
      light: [
        [0.00, [255, 228, 233,   0]],
        [0.18, [255, 205, 215, 150]],
        [0.38, [253, 164, 175, 198]],
        [0.58, [251, 113, 133, 222]],
        [0.80, [225,  29,  72, 236]],
        [1.00, [159,  18,  57, 246]]
      ],
      dark: [
        [0.00, [ 88,  22,  48,   0]],
        [0.18, [136,  30,  68, 150]],
        [0.38, [190,  35,  84, 196]],
        [0.58, [225,  45,  92, 220]],
        [0.80, [255,  77, 109, 238]],
        [1.00, [255, 158, 176, 250]]
      ]
    },
    safe: {
      light: [
        [0.00, [214, 242, 230,   0]],
        [0.18, [176, 233, 208, 148]],
        [0.38, [118, 214, 178, 194]],
        [0.58, [ 52, 190, 145, 218]],
        [0.80, [  8, 145, 109, 234]],
        [1.00, [  4, 100,  75, 246]]
      ],
      dark: [
        [0.00, [ 10,  46,  35,   0]],
        [0.18, [ 18,  84,  62, 148]],
        [0.38, [ 26, 124,  92, 194]],
        [0.58, [ 44, 168, 124, 218]],
        [0.80, [ 74, 212, 158, 236]],
        [1.00, [158, 242, 204, 250]]
      ]
    }
  };

  const ramp = RAMPS[opts.ramp] || RAMPS.distress;
  const stops = opts.gradient ||
    (window.ReUniteTheme ? window.ReUniteTheme.pick(ramp.light, ramp.dark) : ramp.light);

  const peak = opts.peakAlpha || 0.20;   // alpha contributed by one max-weight point

  /* A radius given in metres is converted per repaint from the current
     metres-per-pixel, so the mark stays true to its real footprint instead
     of being a fixed blob on screen. A floor keeps it visible: 5 m is well
     under a pixel at city zoom, and an invisible layer is a broken one. */
  const radiusMeters = opts.radiusMeters || 0;
  const minRadiusPx  = opts.minRadiusPx || 5;

  /* Scales the pixel radius of an area field. The safe layer runs below 1
     so a refuge reads as tighter ground than a flood, without the two
     layers looking like different kinds of thing. */
  const radiusScale = opts.radiusScale || 1;
  const pad = 0.22;                          // render this much beyond the viewport

  /* 256-entry lookup table built once from the ramp */
  const LUT = (function () {
    const t = new Uint8ClampedArray(256 * 4);
    for (let i = 0; i < 256; i++) {
      const v = i / 255;
      let a = stops[0], b = stops[stops.length - 1];
      for (let s = 0; s < stops.length - 1; s++) {
        if (v >= stops[s][0] && v <= stops[s + 1][0]) { a = stops[s]; b = stops[s + 1]; break; }
      }
      const span = (b[0] - a[0]) || 1;
      const k = (v - a[0]) / span;
      for (let c = 0; c < 4; c++) t[i * 4 + c] = a[1][c] + (b[1][c] - a[1][c]) * k;
    }
    return t;
  })();

  /* stable pseudo-random in [0,1) from an integer */
  function hash(n) {
    const x = Math.sin(n * 12.9898 + 78.233) * 43758.5453;
    return x - Math.floor(x);
  }
  function hash2(x, y, seed) {
    const n = Math.sin(x * 127.1 + y * 311.7 + seed * 74.7) * 43758.5453;
    return n - Math.floor(n);
  }

  /* A value-noise lattice, built once per repaint at a coarse resolution.
     Sampling it per pixel is cheap because the grid itself is tiny. The
     lattice is anchored in div-pixel space, so the pattern stays pinned to
     the map while panning. */
  function noiseGrid(ox, oy, cell, gw, gh, seed) {
    const g = new Float32Array(gw * gh);
    for (let j = 0; j < gh; j++) {
      for (let i = 0; i < gw; i++) {
        g[j * gw + i] = hash2(Math.floor(ox / cell) + i, Math.floor(oy / cell) + j, seed);
      }
    }
    return g;
  }

  const Layer = function () {};
  Layer.prototype = new google.maps.OverlayView();

  Layer.prototype.onAdd = function () {
    this.canvas = document.createElement('canvas');
    this.canvas.style.position = 'absolute';
    this.canvas.style.pointerEvents = 'none';
    this.getPanes().overlayLayer.appendChild(this.canvas);

    const self = this;
    const map = this.getMap();
    /* Repaint only when the map settles. Between repaints the painted
       image is scaled by CSS about its anchor, so the hotspots stay locked
       to their coordinates while the user zooms. */
    this._l = [map.addListener('idle', function () { self.render(); })];
  };

  Layer.prototype.onRemove = function () {
    (this._l || []).forEach(function (l) { l.remove(); });
    if (this.canvas && this.canvas.parentNode) this.canvas.parentNode.removeChild(this.canvas);
    this.canvas = null;
  };

  /* Google calls draw() on every transform change. Repainting there would
     be far too expensive, so draw() only re-pegs the canvas to its anchor
     and the actual paint happens on idle. */
  Layer.prototype.draw = function () {
    if (!this._painted) { this.render(); return; }
    this.reposition();
  };

  /* Re-peg the painted image to its anchor. Panning only translates the
     overlay pane, so position alone is enough there — but a zoom changes
     the pixels-per-degree, and moving a canvas painted at the old scale is
     what makes the heat slide off the pins. Scaling it about the anchor
     keeps every hotspot fixed on its coordinate until the next repaint. */
  /* Re-peg the painted image to the map.

     Panning only translates the overlay pane, so position alone is enough
     there. A zoom changes pixels-per-degree, and moving a canvas painted at
     the old scale without rescaling it is what makes the heat slide off the
     markers.

     The scale is measured from the projection itself — the div-pixel span
     between two fixed coordinates now, over the same span when the image
     was painted. Reading map.getZoom() instead is unreliable: it snaps to
     the target zoom while the tiles are still animating, so the overlay
     jumps ahead of the map and visibly drifts through the gesture. */
  Layer.prototype.span = function (proj) {
    const a = proj.fromLatLngToDivPixel(this.anchor);
    const b = proj.fromLatLngToDivPixel(this.ref);
    return { a: a, d: Math.abs(b.x - a.x) || 1 };
  };

  Layer.prototype.reposition = function () {
    const proj = this.getProjection();
    if (!proj || !this.canvas || !this.anchor || !this.paintSpan) return;

    const s = this.span(proj);
    const k = s.d / this.paintSpan;
    const c = this.canvas;

    c.style.left = (s.a.x - c.width / 2) + 'px';
    c.style.top = (s.a.y - c.height / 2) + 'px';
    c.style.transformOrigin = '50% 50%';
    c.style.transform = Math.abs(k - 1) < 0.0005 ? '' : 'scale(' + k + ')';
  };

  Layer.prototype.render = function () {
    const map = this.getMap();
    const proj = this.getProjection();
    if (!map || !proj || !this.canvas) return;

    /* Size from the container, not from getBounds() — after a programmatic
       pan the two disagree and the canvas ends up offset from the viewport,
       clipping the heat along a hard rectangular edge. */
    const div = map.getDiv();
    const cw = div.offsetWidth, ch = div.offsetHeight;
    if (!cw || !ch) return;

    const w = Math.round(cw * (1 + pad * 2));
    const h = Math.round(ch * (1 + pad * 2));
    if (w < 2 || h < 2 || w > 6000 || h > 6000) return;

    this.canvas.width = w;
    this.canvas.height = h;

    /* the map centre anchors the canvas, so panning just moves it; a second
       reference point a fixed number of degrees east gives the scale */
    this.anchor = map.getCenter();
    const lng = this.anchor.lng();
    this.ref = new google.maps.LatLng(
      this.anchor.lat(),
      lng > 179 ? lng - 0.05 : lng + 0.05
    );
    this.canvas.style.transform = '';

    const s0 = this.span(proj);
    this.paintSpan = s0.d;
    const a = s0.a;
    const left = a.x - w / 2, top = a.y - h / 2;
    this.canvas.style.left = left + 'px';
    this.canvas.style.top = top + 'px';

    const ctx = this.canvas.getContext('2d', { willReadFrequently: true });
    ctx.clearRect(0, 0, w, h);

    const z = map.getZoom();
    let r, lobes;
    if (radiusMeters) {
      const lat = this.anchor.lat() * Math.PI / 180;
      const mpp = 156543.03392 * Math.cos(lat) / Math.pow(2, z);
      r = Math.max(minRadiusPx, radiusMeters / mpp);
      lobes = 1;                       /* a point report needs no spread */
    } else {
      r = Math.max(18, Math.min(92, 32 + (z - 9) * 11)) * radiusScale;
      lobes = 4;
    }

    /* 1. Accumulate density into the alpha channel.

       A single radial gradient per point renders a perfect circle, which
       reads as a graphic rather than a spreading field. Each point is
       instead drawn as several offset, unequally-sized lobes, so blobs
       grow irregular edges and merge organically. The offsets are hashed
       from the point index, so the shape is stable across re-renders
       instead of shimmering on every pan. */
    const LOBES = lobes;
    const reach = r * 1.7;                  // furthest a lobe can land
    let drawn = 0;

    for (let i = 0; i < points.length; i++) {
      const p = points[i];
      const d = proj.fromLatLngToDivPixel(new google.maps.LatLng(p.lat, p.lng));
      const x = d.x - left, y = d.y - top;
      if (x < -reach || y < -reach || x > w + reach || y > h + reach) continue;

      /* keep a single point faint so density, not one node, drives colour */
      const alpha = LOBES === 1
        ? Math.min(0.98, peak * 7.0 * Math.min(1, (p.weight || 1) / 3))
        : (peak * Math.min(1, (p.weight || 1) / 3)) / (LOBES * 0.62);

      for (let L = 0; L < LOBES; L++) {
        const h1 = hash(i * 7 + L * 131);
        const h2 = hash(i * 13 + L * 271);
        const h3 = hash(i * 29 + L * 419);
        const ang = h1 * Math.PI * 2;
        const dist = LOBES === 1 ? 0 : h2 * r * 0.62;
        const lr = LOBES === 1 ? r : r * (0.48 + h3 * 0.62);
        const lx = x + Math.cos(ang) * dist;
        const ly = y + Math.sin(ang) * dist * 0.82;   // slightly flattened

        const g = ctx.createRadialGradient(lx, ly, 0, lx, ly, lr);
        g.addColorStop(0, 'rgba(0,0,0,' + alpha.toFixed(4) + ')');
        if (LOBES === 1) g.addColorStop(0.55, 'rgba(0,0,0,' + (alpha * 0.75).toFixed(4) + ')');
        g.addColorStop(1, 'rgba(0,0,0,0)');
        ctx.fillStyle = g;
        ctx.beginPath();
        ctx.arc(lx, ly, lr, 0, Math.PI * 2);
        ctx.fill();
      }
      drawn++;
    }

    /* 2. Warp the density through value noise, then map it through the
       colour ramp.

       Without this the iso-contours stay smooth and every blob reads as a
       circle no matter how the points are scattered. The warp is scaled by
       (1 - density²) so it bites hardest at the edges, where the outline
       is, and leaves the saturated cores under each pin untouched. */
    if (drawn) {
      const CELL = 68;
      const gw = Math.ceil(w / CELL) + 2, gh = Math.ceil(h / CELL) + 2;
      const g1 = noiseGrid(left, top, CELL, gw, gh, 1);

      const CELL2 = 27;
      const gw2 = Math.ceil(w / CELL2) + 2, gh2 = Math.ceil(h / CELL2) + 2;
      const g2 = noiseGrid(left, top, CELL2, gw2, gh2, 7);

      function sample(grid, gwid, cell, x, y) {
        const fx = x / cell, fy = y / cell;
        const ix = fx | 0, iy = fy | 0;
        const tx = fx - ix, ty = fy - iy;
        const u = tx * tx * (3 - 2 * tx), v = ty * ty * (3 - 2 * ty);
        const i0 = iy * gwid + ix, i1 = i0 + gwid;
        const a = grid[i0], b = grid[i0 + 1], c = grid[i1], d = grid[i1 + 1];
        return (a + (b - a) * u) * (1 - v) + (c + (d - c) * u) * v;
      }

      const img = ctx.getImageData(0, 0, w, h);
      const px = img.data;
      for (let y = 0; y < h; y++) {
        for (let x = 0; x < w; x++) {
          const i = (y * w + x) * 4 + 3;
          const av = px[i];
          if (!av) continue;

          const nz = sample(g1, gw, CELL, x, y) * 0.68 +
                     sample(g2, gw2, CELL2, x, y) * 0.32;
          const dens = av / 255;
          const m = radiusMeters ? 1 : 1 + (nz - 0.5) * 1.35 * (1 - dens * dens);
          let o = av * m;
          o = o < 0 ? 0 : o > 255 ? 255 : o;

          const k = (o | 0) * 4;
          px[i - 3] = LUT[k];
          px[i - 2] = LUT[k + 1];
          px[i - 1] = LUT[k + 2];
          px[i]     = LUT[k + 3];
        }
      }
      ctx.putImageData(img, 0, 0);
    }
    this._painted = true;
  };

  Layer.prototype.setVisible = function (on) {
    if (this.canvas) this.canvas.style.display = on ? '' : 'none';
  };

  return new Layer();
};
