/* ════════════════════════════════════════════════════════════
   ReUnite — Earth backdrop for the landing hero

   A shaded, textured globe rather than a dot matrix. An
   equirectangular Earth texture is painted once into an offscreen
   canvas — ocean bathymetry, land filled as paths, biome bands by
   latitude, ice caps, grain — and each frame the visible hemisphere
   is sampled through an inverse orthographic projection and lit with
   a diffuse term.

   The sphere rebuilds at a capped rate, since the rotation is slow;
   the beacons, relay arcs and atmosphere composite over it at full
   frame rate. Frozen on the first frame under prefers-reduced-motion.
   ════════════════════════════════════════════════════════════ */
(function () {
  'use strict';

  const cv = document.getElementById('globe');
  if (!cv || !cv.getContext) return;
  const ctx = cv.getContext('2d');
  const REDUCED = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  const THEME = window.ReUniteTheme;
  const PALETTE = {
    light: {
      deep:'#17456B', shelf:'#2C74A0', shallow:'#4BA0C4',
      land:'#4E7A4B', arid:'#B79463', tropic:'#3B7440', tundra:'#8D9C88', ice:'#EDF4F8',
      grat:'rgba(255,255,255,0.15)',
      rim:'rgba(120,175,225,0.45)', rimOut:'rgba(120,175,225,0)',
      sos:'#E11D48', arc:'rgba(225,29,72,0.8)', ring:'225,29,72',
      pin:'#ffffff', ambient:0.36, gain:0.84, photoGain:1.18
    },
    dark: {
      deep:'#0A2035', shelf:'#123D5A', shallow:'#1A5878',
      land:'#2C4C33', arid:'#6A5537', tropic:'#254829', tundra:'#4C584F', ice:'#B4C7D3',
      grat:'rgba(180,215,245,0.11)',
      rim:'rgba(90,150,225,0.38)', rimOut:'rgba(90,150,225,0)',
      sos:'#FF5C7A', arc:'rgba(255,92,122,0.85)', ring:'255,92,122',
      pin:'rgba(255,255,255,.9)', ambient:0.17, gain:0.95, photoGain:0.86
    }
  };
  let C = PALETTE[(THEME && THEME.isDark) ? 'dark' : 'light'];

  /* Coarse continent outlines in [lon, lat], filled as paths so the
     coastlines read as land masses rather than a stipple. */
  const LAND = [
    /* Africa */
    [[-17,14],[-16,21],[-12,27],[-9,31],[-6,36],[2,37],[11,37],[20,33],[25,32],[32,31],[35,24],[38,18],[43,12],[51,12],[51,5],[45,0],[41,-5],[40,-11],[36,-18],[33,-25],[29,-31],[25,-34],[19,-35],[15,-27],[12,-17],[9,-5],[9,4],[3,6],[-5,5],[-10,6],[-14,9],[-17,14]],
    /* Europe + Asia */
    [[-10,36],[-9,43],[-2,43],[-1,48],[-5,49],[2,51],[4,53],[8,54],[8,57],[13,55],[19,54],[21,56],[24,60],[21,65],[24,70],[31,70],[40,67],[50,68],[60,70],[68,72],[76,73],[85,74],[100,77],[113,74],[128,73],[140,73],[152,70],[162,68],[170,66],[179,65],[179,62],[163,60],[157,52],[145,44],[135,44],[130,42],[127,35],[122,31],[120,24],[110,20],[106,10],[100,3],[97,8],[95,16],[89,22],[80,15],[77,8],[73,18],[68,23],[62,25],[56,26],[50,29],[43,37],[36,36],[29,41],[26,38],[23,36],[16,38],[13,45],[9,44],[3,43],[-3,36],[-10,36]],
    /* South-east Asian arc */
    [[95,5],[105,1],[115,-3],[125,-2],[135,-3],[141,-8],[132,-8],[120,-9],[110,-7],[100,-1],[95,5]],
    /* North America */
    [[-168,66],[-160,71],[-148,70],[-133,69],[-122,70],[-110,68],[-95,68],[-85,67],[-80,73],[-70,70],[-62,60],[-56,52],[-64,46],[-70,42],[-74,39],[-76,35],[-81,26],[-84,30],[-90,29],[-97,26],[-98,19],[-95,16],[-88,16],[-84,10],[-79,9],[-83,15],[-92,15],[-105,20],[-110,24],[-117,32],[-124,40],[-125,48],[-131,54],[-140,60],[-152,59],[-160,56],[-166,60],[-168,66]],
    /* Greenland */
    [[-45,60],[-52,64],[-54,70],[-58,75],[-50,80],[-30,83],[-20,80],[-22,73],[-33,68],[-42,61],[-45,60]],
    /* South America */
    [[-81,-4],[-79,2],[-75,10],[-68,11],[-61,9],[-52,4],[-44,-2],[-35,-6],[-38,-14],[-46,-24],[-53,-33],[-58,-38],[-62,-41],[-66,-46],[-70,-53],[-75,-52],[-73,-44],[-72,-35],[-71,-25],[-70,-18],[-75,-14],[-79,-6],[-81,-4]],
    /* Australia */
    [[113,-22],[114,-26],[118,-34],[126,-32],[134,-32],[138,-35],[145,-38],[150,-37],[153,-28],[148,-20],[142,-11],[135,-12],[130,-11],[125,-14],[118,-20],[113,-22]],
    /* New Zealand */
    [[172,-41],[174,-37],[178,-38],[176,-41],[171,-45],[167,-46],[172,-41]],
    /* Antarctica */
    [[-180,-72],[-150,-75],[-110,-74],[-70,-72],[-60,-64],[-30,-70],[10,-70],[40,-68],[80,-67],[120,-66],[150,-72],[180,-72],[180,-90],[-180,-90],[-180,-72]]
  ];

  function hash(n) {
    const x = Math.sin(n * 12.9898 + 78.233) * 43758.5453;
    return x - Math.floor(x);
  }

  /* ── the Earth texture, equirectangular ─────────────────
     NASA Blue Marble (public domain) is used when it loads. The
     procedural texture below stays as the fallback, so the hero still
     renders a globe if the image is missing or blocked. */
  const TW = 1024, TH = 512;
  let texData = null, photo = false;

  function loadPhoto() {
    const img = new Image();
    img.decoding = 'async';
    img.onload = function () {
      const t = document.createElement('canvas');
      t.width = TW; t.height = TH;
      const g = t.getContext('2d', { willReadFrequently: true });
      g.drawImage(img, 0, 0, TW, TH);

      /* Blue Marble is already bright, so only a whisper of lift in light
         mode to stop the oceans reading as a dark hole on a pale page. */
      if (!(THEME && THEME.isDark)) {
        g.globalCompositeOperation = 'lighten';
        g.fillStyle = 'rgba(120,160,200,0.10)';
        g.fillRect(0, 0, TW, TH);
        g.globalCompositeOperation = 'source-over';
      }
      texData = g.getImageData(0, 0, TW, TH).data;
      photo = true;
      renderSphere();
      if (REDUCED) draw(performance.now());
    };
    img.onerror = function () {
      console.warn('[ReUnite] Earth texture unavailable, using the procedural globe');
      photo = false;
      buildTexture();
      renderSphere();
      if (REDUCED) draw(performance.now());
    };
    img.src = 'assets/img/earth.jpg';
  }

  function buildTexture() {
    const t = document.createElement('canvas');
    t.width = TW; t.height = TH;
    const g = t.getContext('2d', { willReadFrequently: true });

    const X = function (lon) { return (lon + 180) / 360 * TW; };
    const Y = function (lat) { return (90 - lat) / 180 * TH; };

    /* ocean: deep toward the poles, lighter through the tropics */
    const sea = g.createLinearGradient(0, 0, 0, TH);
    sea.addColorStop(0, C.deep);
    sea.addColorStop(0.30, C.shelf);
    sea.addColorStop(0.50, C.shallow);
    sea.addColorStop(0.70, C.shelf);
    sea.addColorStop(1, C.deep);
    g.fillStyle = sea;
    g.fillRect(0, 0, TW, TH);

    /* broad basins, so the ocean is not a flat band */
    for (let i = 0; i < 30; i++) {
      const cx = hash(i * 3 + 1) * TW, cy = hash(i * 7 + 2) * TH;
      const rr = (0.05 + hash(i * 11 + 3) * 0.15) * TW;
      const rg = g.createRadialGradient(cx, cy, 0, cx, cy, rr);
      rg.addColorStop(0, 'rgba(0,0,0,0.17)');
      rg.addColorStop(1, 'rgba(0,0,0,0)');
      g.fillStyle = rg;
      g.fillRect(cx - rr, cy - rr, rr * 2, rr * 2);
    }

    /* Land is built on its own transparent layer. Painting the biome
       bands straight onto the main canvas with source-atop would tint the
       ocean too — every pixel is already opaque once the sea is filled. */
    const lc = document.createElement('canvas');
    lc.width = TW; lc.height = TH;
    const lg = lc.getContext('2d');

    lg.fillStyle = C.land;
    LAND.forEach(function (poly) {
      lg.beginPath();
      poly.forEach(function (p, i) {
        const x = X(p[0]), y = Y(p[1]);
        if (i === 0) lg.moveTo(x, y); else lg.lineTo(x, y);
      });
      lg.closePath();
      lg.fill();
    });

    /* biome bands, clipped to the land silhouette */
    lg.save();
    lg.globalCompositeOperation = 'source-atop';
    const bio = lg.createLinearGradient(0, 0, 0, TH);
    bio.addColorStop(0.00, C.ice);
    bio.addColorStop(0.13, C.tundra);
    bio.addColorStop(0.26, C.land);
    bio.addColorStop(0.36, C.arid);
    bio.addColorStop(0.47, C.tropic);
    bio.addColorStop(0.56, C.tropic);
    bio.addColorStop(0.66, C.arid);
    bio.addColorStop(0.78, C.land);
    bio.addColorStop(0.90, C.tundra);
    bio.addColorStop(1.00, C.ice);
    lg.globalAlpha = 0.86;
    lg.fillStyle = bio;
    lg.fillRect(0, 0, TW, TH);
    lg.restore();

    /* a soft coastal shelf, then the land itself */
    g.save();
    g.globalAlpha = 0.5;
    g.filter = 'blur(6px)';
    g.drawImage(lc, 0, 0);
    g.restore();
    g.drawImage(lc, 0, 0);

    /* polar ice, over land and sea alike */
    const capN = g.createLinearGradient(0, 0, 0, Y(58));
    capN.addColorStop(0, C.ice);
    capN.addColorStop(1, 'rgba(255,255,255,0)');
    g.fillStyle = capN; g.fillRect(0, 0, TW, Y(58));
    const capS = g.createLinearGradient(0, TH, 0, Y(-60));
    capS.addColorStop(0, C.ice);
    capS.addColorStop(1, 'rgba(255,255,255,0)');
    g.fillStyle = capS; g.fillRect(0, Y(-60), TW, TH - Y(-60));

    /* grain, so the surface is not flat colour */
    const n = document.createElement('canvas');
    n.width = 256; n.height = 128;
    const ng = n.getContext('2d');
    const nd = ng.createImageData(256, 128);
    for (let i = 0; i < nd.data.length; i += 4) {
      const v = 118 + Math.floor(hash(i) * 140);
      nd.data[i] = nd.data[i + 1] = nd.data[i + 2] = v;
      nd.data[i + 3] = 255;
    }
    ng.putImageData(nd, 0, 0);
    g.save();
    g.globalCompositeOperation = 'overlay';
    g.globalAlpha = 0.22;
    g.drawImage(n, 0, 0, TW, TH);
    g.restore();

    texData = g.getImageData(0, 0, TW, TH).data;
  }

  /* ── the mesh itself ─────────────────────────────────────
     A dense scatter of nodes on land rather than a handful of cities.
     Relays only ever run between near neighbours, so the arcs are short
     hops — which is what a mesh actually looks like, and reads far
     better than a few continent-spanning lines. */
  function onLand(lon, lat) {
    for (let p = 0; p < LAND.length; p++) {
      const poly = LAND[p];
      let inside = false;
      for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
        const xi = poly[i][0], yi = poly[i][1], xj = poly[j][0], yj = poly[j][1];
        if ((yi > lat) !== (yj > lat) &&
            lon < (xj - xi) * (lat - yi) / (yj - yi) + xi) inside = !inside;
      }
      if (inside) return true;
    }
    return false;
  }

  const SITES = (function () {
    /* The spiral walks from one pole to the other, so collecting land hits
       until a cap is reached would put every node in the far north. Sample
       the whole sphere first, then thin the result evenly. */
    const all = [];
    const N = 4200;
    const golden = Math.PI * (3 - Math.sqrt(5));
    for (let i = 0; i < N; i++) {
      const y = 1 - (i / (N - 1)) * 2;
      const r = Math.sqrt(Math.max(0, 1 - y * y));
      const th = golden * i;
      const lat = Math.asin(y) * 180 / Math.PI;
      const lon = Math.atan2(Math.sin(th) * r, Math.cos(th) * r) * 180 / Math.PI;
      /* skip the ice sheets — nobody is meshing across them */
      if (Math.abs(lat) > 68) continue;
      if (onLand(lon, lat)) all.push({ lon: lon, lat: lat });
    }
    const want = 96;
    if (all.length <= want) return all;
    const step = all.length / want, out = [];
    for (let i = 0; i < want; i++) out.push(all[Math.floor(i * step)]);
    return out;
  })();

  /* Unit vector per node, so arcs can be interpolated on the sphere
     itself. Linear lon/lat interpolation is fine for a short hop but
     visibly leaves the surface once the span gets large. */
  const VEC = SITES.map(function (s) {
    const p = s.lat * Math.PI / 180, l = s.lon * Math.PI / 180;
    return [Math.cos(p) * Math.sin(l), Math.sin(p), Math.cos(p) * Math.cos(l)];
  });

  /* Partners are chosen by great-circle distance: far enough to sweep a
     long way across the globe, short of the far side where the arc would
     disappear behind it. */
  const NEAR = VEC.map(function (a, ai) {
    const list = [];
    VEC.forEach(function (b, bi) {
      if (ai === bi) return;
      let d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
      d = d > 1 ? 1 : d < -1 ? -1 : d;
      const ang = Math.acos(d);
      if (ang > 0.60 && ang < 1.85) list.push(bi);   // ≈34° to ≈106°
    });
    return list;
  });

  function slerp(a, b, t) {
    let d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    d = d > 1 ? 1 : d < -1 ? -1 : d;
    const th = Math.acos(d);
    if (th < 1e-6) return a;
    const s2 = Math.sin(th);
    const w1 = Math.sin((1 - t) * th) / s2, w2 = Math.sin(t * th) / s2;
    return [a[0] * w1 + b[0] * w2, a[1] * w1 + b[1] * w2, a[2] * w1 + b[2] * w2];
  }

  let W = 0, H = 0, DPR = 1, R = 0, CX = 0, CY = 0;
  let spin = -0.35;
  let pulses = [];

  /* the lit hemisphere renders offscreen at reduced resolution and is
     upscaled — the softness reads as atmosphere and costs a third of
     the pixels */
  const SPHERE_SCALE = 0.62;
  let sphere = null, sctx = null, sR = 0, lastSphere = 0;

  function resize() {
    DPR = Math.min(window.devicePixelRatio || 1, 2);
    const rect = cv.getBoundingClientRect();
    W = rect.width; H = rect.height;
    cv.width = Math.round(W * DPR);
    cv.height = Math.round(H * DPR);
    ctx.setTransform(DPR, 0, 0, DPR, 0, 0);

    if (W < 760) {
      R  = Math.min(W * 0.62, H * 0.40);
      CX = W * 0.62;
      CY = H + R * 0.58;
    } else if (W < 1020) {
      /* not much room between the copy and the globe at these widths, so
         push more of the sphere off the right edge */
      R  = Math.min(W * 0.32, H * 0.46);
      CX = W * 0.90;
      CY = H * 0.52;
    } else {
      R  = Math.min(W * 0.27, H * 0.42);
      CX = W * 0.765;
      CY = H * 0.53;
    }

    sR = Math.max(24, Math.round(R * SPHERE_SCALE));
    if (!sphere) sphere = document.createElement('canvas');
    sphere.width = sR * 2; sphere.height = sR * 2;
    sctx = sphere.getContext('2d', { willReadFrequently: true });
    lastSphere = 0;
  }

  /* ── the sphere itself ───────────────────────────────── */
  const LX = -0.48, LY = 0.55, LZ = 0.68;          // light direction
  const LN = Math.sqrt(LX * LX + LY * LY + LZ * LZ);

  function renderSphere() {
    if (!texData || !sctx) return;
    const S = sR * 2;
    const img = sctx.createImageData(S, S);
    const px = img.data;

    for (let y = 0; y < S; y++) {
      const py = (y - sR) / sR;
      for (let x = 0; x < S; x++) {
        const pxn = (x - sR) / sR;
        const r2 = pxn * pxn + py * py;
        if (r2 > 1) continue;

        const nz = Math.sqrt(1 - r2);
        const wy = -py;                              // screen y is flipped

        /* inverse orthographic, undoing the spin about the polar axis */
        const lat = Math.asin(wy);
        const lon = Math.atan2(pxn, nz) - spin;

        const tx = ((((lon * 57.29577951 + 180) % 360) + 360) % 360) / 360 * TW;
        const ty = (90 - lat * 57.29577951) / 180 * TH;
        const ti = (((ty | 0) * TW) + (tx | 0)) * 4;

        /* diffuse shading with a soft falloff toward the limb */
        let lit;
        if (photo) {
          /* the imagery is already lit, so only shade the limb and dim
             the whole sphere a touch in dark mode */
          lit = (0.55 + 0.45 * nz) * (C.photoGain);
        } else {
          let d = (pxn * LX + wy * LY + nz * LZ) / LN;
          if (d < 0) d = 0;
          lit = C.ambient + C.gain * d * d * (0.55 + 0.45 * nz);
        }

        const o = (y * S + x) * 4;
        px[o]     = texData[ti] * lit;
        px[o + 1] = texData[ti + 1] * lit;
        px[o + 2] = texData[ti + 2] * lit;
        px[o + 3] = 255;
      }
    }
    sctx.putImageData(img, 0, 0);
  }

  function project(lon, lat) {
    const p = lat * Math.PI / 180;
    const l = lon * Math.PI / 180 + spin;
    return {
      x: CX + Math.cos(p) * Math.sin(l) * R,
      y: CY - Math.sin(p) * R,
      z: Math.cos(p) * Math.cos(l)
    };
  }

  function projectVec(v) {
    const cs = Math.cos(spin), sn = Math.sin(spin);
    const x = v[0] * cs + v[2] * sn;
    const z = -v[0] * sn + v[2] * cs;
    return { x: CX + x * R, y: CY - v[1] * R, z: z };
  }

  function drawGraticule() {
    ctx.strokeStyle = C.grat;
    ctx.lineWidth = 1;
    for (let lat = -60; lat <= 60; lat += 30) {
      ctx.beginPath();
      let started = false;
      for (let lon = -180; lon <= 180; lon += 3) {
        const p = project(lon, lat);
        if (p.z <= 0) { started = false; continue; }
        if (!started) { ctx.moveTo(p.x, p.y); started = true; } else ctx.lineTo(p.x, p.y);
      }
      ctx.stroke();
    }
    for (let lon = -180; lon < 180; lon += 30) {
      ctx.beginPath();
      let started = false;
      for (let lat = -90; lat <= 90; lat += 3) {
        const p = project(lon, lat);
        if (p.z <= 0) { started = false; continue; }
        if (!started) { ctx.moveTo(p.x, p.y); started = true; } else ctx.lineTo(p.x, p.y);
      }
      ctx.stroke();
    }
  }

  function drawArc(ai, bi, k, fade) {
    const steps = 44;                       /* long spans need resolution */
    const a = VEC[ai], b = VEC[bi];
    ctx.strokeStyle = C.arc;
    ctx.globalAlpha = fade === undefined ? 1 : Math.max(0, Math.min(1, fade));
    ctx.lineWidth = 1.8;
    ctx.beginPath();
    let started = false;
    const upto = Math.floor(steps * k);
    for (let i = 0; i <= upto; i++) {
      const p = projectVec(slerp(a, b, i / steps));
      if (p.z <= 0.02) { started = false; continue; }
      if (!started) { ctx.moveTo(p.x, p.y); started = true; } else ctx.lineTo(p.x, p.y);
    }
    ctx.stroke();
    ctx.globalAlpha = 1;
  }

  function draw(now) {
    ctx.clearRect(0, 0, W, H);

    /* atmosphere */
    const glow = ctx.createRadialGradient(CX, CY, R * 0.93, CX, CY, R * 1.18);
    glow.addColorStop(0, C.rim);
    glow.addColorStop(1, C.rimOut);
    ctx.fillStyle = glow;
    ctx.beginPath(); ctx.arc(CX, CY, R * 1.18, 0, Math.PI * 2); ctx.fill();

    /* the globe, upscaled from the offscreen render */
    if (sphere) {
      ctx.save();
      ctx.beginPath(); ctx.arc(CX, CY, R, 0, Math.PI * 2); ctx.clip();
      ctx.imageSmoothingEnabled = true;
      ctx.imageSmoothingQuality = 'high';
      ctx.drawImage(sphere, CX - R, CY - R, R * 2, R * 2);
      ctx.restore();
    }

    drawGraticule();

    /* beacons and their relays */
    pulses = pulses.filter(function (q) { return now - q.t < PULSE_MS; });
    pulses.forEach(function (q) {
      const k = (now - q.t) / PULSE_MS;
      const a = SITES[q.a];
      drawArc(q.a, q.b, Math.min(1, k / 0.5), k < 0.55 ? 1 : (1 - k) / 0.45);
      const pa = project(a.lon, a.lat);
      if (pa.z > 0.02) {
        ctx.beginPath();
        ctx.arc(pa.x, pa.y, 2.5 + k * R * 0.055, 0, Math.PI * 2);
        ctx.strokeStyle = 'rgba(' + C.ring + ',' + (0.6 * (1 - k)).toFixed(3) + ')';
        ctx.lineWidth = 1.2;
        ctx.stroke();
      }
    });

    ctx.fillStyle = C.sos;
    SITES.forEach(function (s) {
      const p = project(s.lon, s.lat);
      if (p.z <= 0.04) return;
      ctx.globalAlpha = Math.min(1, 0.32 + p.z * 0.78);
      ctx.beginPath(); ctx.arc(p.x, p.y, 1.8, 0, Math.PI * 2);
      ctx.fill();
    });
    ctx.globalAlpha = 1;
  }

  /* ── loop ────────────────────────────────────────────── */
  const PULSE_MS = 4200;        /* how long one relay stays on screen */
  const SPAWN_MS = 620;         /* sparse: roughly seven arcs live at once */
  let lastPulse = 0;
  function tick(now) {
    spin += 0.00042;
    /* the sphere only needs rebuilding as fast as it visibly turns */
    if (now - lastSphere > 55) { renderSphere(); lastSphere = now; }
    if (now - lastPulse > SPAWN_MS && SITES.length) {
      lastPulse = now;
      /* relay to a near neighbour, so the hop stays short */
      const a = Math.floor(Math.random() * SITES.length);
      const n = NEAR[a];
      if (n && n.length) {
        pulses.push({ a: a, b: n[Math.floor(Math.random() * n.length)], t: now });
      }
    }
    draw(now);
    requestAnimationFrame(tick);
  }

  document.addEventListener('reunite:theme', function (e) {
    C = PALETTE[e.detail.name === 'dark' ? 'dark' : 'light'];
    if (photo) { renderSphere(); } else { buildTexture(); renderSphere(); }
    if (REDUCED) draw(performance.now());
  });

  let rt;
  window.addEventListener('resize', function () {
    clearTimeout(rt);
    rt = setTimeout(function () {
      resize(); renderSphere();
      if (REDUCED) draw(performance.now());
    }, 140);
  });

  resize();
  buildTexture();          /* something on screen immediately … */
  renderSphere();
  loadPhoto();             /* … then upgrade to the photograph */
  if (REDUCED) draw(performance.now());
  else requestAnimationFrame(tick);
})();
