/* ════════════════════════════════════════════════════════════
   ReUnite — simulated mesh dataset (Greater Sydney)
   Everything here is generated for demonstration only.
   No real distress signal is represented.
   ════════════════════════════════════════════════════════════ */
window.ReUniteData = (function () {
  'use strict';

  /* deterministic PRNG so the heatmap looks identical on every load */
  function mulberry32(a) {
    return function () {
      a |= 0; a = (a + 0x6D2B79F5) | 0;
      let t = Math.imul(a ^ (a >>> 15), 1 | a);
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }
  const rnd = mulberry32(20260830);

  /* box-muller for believable neighbourhood spread */
  function gauss() {
    let u = 0, v = 0;
    while (u === 0) u = rnd();
    while (v === 0) v = rnd();
    return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
  }

  const CENTER = [150.9800, -33.8300];   // Greater Sydney, Hawkesbury–Nepean corridor

  /* ── named SOS signals (the readable log) ─────────────── */
  const SIGNALS = [
    { id:'SOS-4417', who:'Node 4417',  prio:'medical',    msg:'Insulin gone, two diabetic adults. Second floor, water at knee height.', needs:['Medical','Water'], hops:3, ttl:7,  batt:41, age:2,  lon:150.8121, lat:-33.6042 },
    { id:'SOS-2290', who:'Node 2290',  prio:'trapped',    msg:'Four adults cut off by the Hawkesbury. Can hear us from the levee.',     needs:['Rescue'],           hops:5, ttl:5,  batt:23, age:4,  lon:150.8266, lat:-33.6188 },
    { id:'SOS-8801', who:'Node 8801',  prio:'vulnerable', msg:'Grandmother, 84, cannot walk. Roof is holding but water still rising.',  needs:['Shelter','Medical'],hops:2, ttl:7,  batt:66, age:5,  lon:150.7488, lat:-33.5934 },
    { id:'SOS-6134', who:'Node 6134',  prio:'medical',    msg:'Open leg fracture, bleeding controlled. Needs a boat, not a car.',       needs:['Medical'],          hops:6, ttl:4,  batt:12, age:7,  lon:150.9391, lat:-33.9104 },
    { id:'SOS-3372', who:'Node 3372',  prio:'trapped',    msg:'Nine of us on a roof, no way down. Ladder or boat.',                     needs:['Rescue','Water'],   hops:4, ttl:6,  batt:55, age:9,  lon:150.9527, lat:-33.9231 },
    { id:'SOS-5028', who:'Node 5028',  prio:'vulnerable', msg:'Three children, no adult. Neighbour relaying for them.',                 needs:['Food','Shelter'],   hops:7, ttl:3,  batt:31, age:12, lon:150.6905, lat:-33.7462 },
    { id:'SOS-1156', who:'Node 1156',  prio:'standard',   msg:'Six households safe on high ground. Out of drinking water tomorrow.',    needs:['Water','Food'],     hops:2, ttl:4,  batt:78, age:14, lon:150.9861, lat:-33.9298 },
    { id:'SOS-9043', who:'Node 9043',  prio:'medical',    msg:'Chest pain, 61-year-old male, conscious. Road to hospital is under.',    needs:['Medical'],          hops:5, ttl:5,  batt:19, age:17, lon:150.9212, lat:-33.9247 },
    { id:'SOS-7719', who:'Node 7719',  prio:'trapped',    msg:'Garage flooded, exit blocked by debris. Two adults, one dog.',           needs:['Rescue'],           hops:3, ttl:7,  batt:47, age:21, lon:150.7043, lat:-34.0498 },
    { id:'SOS-2604', who:'Node 2604',  prio:'vulnerable', msg:'Pregnant, 8 months, contractions started. Sheltering in a school hall.', needs:['Medical'],          hops:4, ttl:6,  batt:58, age:24, lon:151.0011, lat:-33.8192 },
    { id:'SOS-4480', who:'Node 4480',  prio:'standard',   msg:'Cluster of 5 phones, all accounted for. Sharing one power bank.',        needs:['Food'],             hops:1, ttl:5,  batt:88, age:28, lon:151.2059, lat:-33.8721 },
    { id:'SOS-3915', who:'Node 3915',  prio:'medical',    msg:'Dialysis patient, missed two sessions. Needs evacuation, not supplies.', needs:['Medical'],          hops:8, ttl:1,  batt:9,  age:33, lon:150.8047, lat:-33.5966 },
    { id:'SOS-6667', who:'Node 6667',  prio:'vulnerable', msg:'Deaf resident alone, texting only. Confirm before entering the house.',  needs:['Rescue'],           hops:6, ttl:4,  batt:37, age:39, lon:150.6812, lat:-34.0611 },
    { id:'SOS-1802', who:'Node 1802',  prio:'trapped',    msg:'Road gone both directions. 14 people, no injuries yet.',                 needs:['Food','Water'],     hops:5, ttl:5,  batt:62, age:44, lon:150.7614, lat:-33.6103 },
    { id:'SOS-5541', who:'Node 5541',  prio:'standard',   msg:'Relaying for the street. Have a generator, can charge phones.',          needs:[],                   hops:2, ttl:6,  batt:94, age:52, lon:150.9702, lat:-33.9401 },
    { id:'SOS-8276', who:'Node 8276',  prio:'vulnerable', msg:'Two wheelchair users on ground floor. Water 20 cm and climbing.',        needs:['Rescue','Shelter'], hops:4, ttl:6,  batt:44, age:61, lon:150.9338, lat:-33.9053 }
  ];

  /* ── heat field, derived from the victim pins ────────────
     Every SOS pin anchors its own cluster: the signal itself at full
     weight, plus the handset cluster that relayed it scattered around.
     The density map is therefore never out of step with the markers —
     each pin always sits on a hot core. */
  const PRIO_WEIGHT = { medical: 3, trapped: 3, vulnerable: 2, standard: 1 };
  const CLUSTER_N   = { medical: 62, trapped: 56, vulnerable: 42, standard: 28 };
  const CLUSTER_SPREAD = 0.019;          // degrees, ≈2 km — wide enough
                                         // that neighbouring clusters merge

  const heatFeatures = [];
  function pushHeat(lon, lat, severity, tag) {
    heatFeatures.push({
      type: 'Feature',
      properties: { severity: Math.max(1, Math.min(3, severity)), cluster: tag },
      geometry: { type: 'Point', coordinates: [+lon.toFixed(5), +lat.toFixed(5)] }
    });
  }

  SIGNALS.forEach(function (s, si) {
    const w = PRIO_WEIGHT[s.prio] || 1;

    /* a tight, saturated core directly under the pin, so the density
       always peaks red exactly where the marker sits */
    for (let c = 0; c < 9; c++) {
      pushHeat(
        s.lon + (rnd() - 0.5) * 0.0016,
        s.lat + (rnd() - 0.5) * 0.0012,
        3, si
      );
    }

    /* The cluster of devices around it. A single isotropic Gaussian
       produces a perfectly round footprint, so each cluster is instead
       built from 3–5 sub-lobes, each offset, rotated and stretched
       differently. The union reads as an irregular affected area rather
       than a circle. */
    const n = CLUSTER_N[s.prio] || 18;
    const nl = 3 + Math.floor(rnd() * 3);
    const lobes = [];
    let share = 0;
    for (let l = 0; l < nl; l++) {
      /* Lobe angles are spread evenly around the pin with jitter, and the
         offset is kept short. Free-floating offsets let the whole mass
         drift off the marker — invisible at metro zoom, glaring at street
         level. Even angles keep the cluster centred while varied radii and
         sizes still break the outline. */
      const ang = (l / nl) * Math.PI * 2 + (rnd() - 0.5) * (Math.PI * 1.6 / nl);
      const dist = (0.12 + rnd() * 0.5) * CLUSTER_SPREAD;
      const sh = 0.45 + rnd();
      share += sh;
      lobes.push({
        lon: s.lon + Math.cos(ang) * dist,
        lat: s.lat + Math.sin(ang) * dist * 0.72,
        rot: rnd() * Math.PI,
        sx: (0.40 + rnd() * 0.85) * CLUSTER_SPREAD,
        sy: (0.28 + rnd() * 0.55) * CLUSTER_SPREAD,
        sh: sh
      });
    }

    lobes.forEach(function (L) {
      const cnt = Math.max(2, Math.round(n * L.sh / share));
      for (let i = 0; i < cnt; i++) {
        const gx = gauss() * L.sx;
        const gy = gauss() * L.sy;
        /* rotate the anisotropic spread so lobes point in varied directions */
        const rx = gx * Math.cos(L.rot) - gy * Math.sin(L.rot);
        const ry = gx * Math.sin(L.rot) + gy * Math.cos(L.rot);
        const d = Math.hypot(gx / L.sx, gy / L.sy) * 0.5;
        pushHeat(L.lon + rx, L.lat + ry * 0.72, Math.round(w - d * 0.85 + (rnd() - 0.5)), si);
      }
    });
  });

  /* a thin ambient scatter of lone nodes between the clusters */
  for (let i = 0; i < 44; i++) {
    pushHeat(
      CENTER[0] + (rnd() - 0.5) * 0.58,
      CENTER[1] + (rnd() - 0.5) * 0.38,
      1, -1
    );
  }

  /* ── safe-place field ────────────────────────────────────
     The counterpart to the distress map: high ground and staffed
     evacuation points, built the same multi-lobe way so the two layers
     read as one system. Confidence falls off with distance from the
     anchor, exactly as distress density does. */
  const SAFE_ANCHORS = [
    { lon: 150.9350, lat: -33.7700, w: 3, n: 52 },  // Seven Hills ridge
    { lon: 151.0060, lat: -33.7300, w: 3, n: 48 },  // Castle Hill plateau
    { lon: 150.6931, lat: -33.7511, w: 3, n: 44 },  // Penrith rescue command
    { lon: 150.9245, lat: -33.9166, w: 3, n: 44 },  // Liverpool medical camp
    { lon: 151.0043, lat: -33.8168, w: 2, n: 36 },  // Parramatta shelter
    { lon: 150.8088, lat: -33.6121, w: 2, n: 34 },  // Windsor evacuation centre
    { lon: 151.2093, lat: -33.8688, w: 2, n: 40 },  // Sydney CBD, above the flats
    { lon: 151.0770, lat: -33.7160, w: 2, n: 32 },  // Hornsby plateau
    { lon: 150.8140, lat: -34.0640, w: 2, n: 30 }   // Campbelltown high ground
  ];

  /* Safe places are areas, built exactly like the distress field — same
     multi-lobe construction, same spread. Only the colour differs, so the
     two layers are directly comparable. */
  const SAFE_SPREAD = CLUSTER_SPREAD;

  const safeFeatures = [];
  function pushSafe(lon, lat, severity, tag) {
    safeFeatures.push({
      type: 'Feature',
      properties: { severity: Math.max(1, Math.min(3, severity)), cluster: tag },
      geometry: { type: 'Point', coordinates: [+lon.toFixed(5), +lat.toFixed(5)] }
    });
  }

  SAFE_ANCHORS.forEach(function (a, ai) {
    /* a tight core, so the anchor itself always reads as the safest point */
    for (let c = 0; c < 9; c++) {
      pushSafe(a.lon + (rnd() - 0.5) * 0.0016, a.lat + (rnd() - 0.5) * 0.0012, 3, ai);
    }

    const nl = 3 + Math.floor(rnd() * 3);
    const lobes = [];
    let share = 0;
    for (let l = 0; l < nl; l++) {
      const ang = (l / nl) * Math.PI * 2 + (rnd() - 0.5) * (Math.PI * 1.6 / nl);
      const dist = (0.12 + rnd() * 0.5) * SAFE_SPREAD;
      const sh = 0.45 + rnd();
      share += sh;
      lobes.push({
        lon: a.lon + Math.cos(ang) * dist,
        lat: a.lat + Math.sin(ang) * dist * 0.72,
        rot: rnd() * Math.PI,
        sx: (0.40 + rnd() * 0.85) * SAFE_SPREAD,
        sy: (0.28 + rnd() * 0.55) * SAFE_SPREAD,
        sh: sh
      });
    }

    lobes.forEach(function (L) {
      const cnt = Math.max(2, Math.round(a.n * L.sh / share));
      for (let i = 0; i < cnt; i++) {
        const gx = gauss() * L.sx, gy = gauss() * L.sy;
        const rx = gx * Math.cos(L.rot) - gy * Math.sin(L.rot);
        const ry = gx * Math.sin(L.rot) + gy * Math.cos(L.rot);
        const d = Math.hypot(gx / L.sx, gy / L.sy) * 0.5;
        pushSafe(L.lon + rx, L.lat + ry * 0.72, Math.round(a.w - d * 0.85 + (rnd() - 0.5)), ai);
      }
    });
  });

  const sosFeatures = SIGNALS.map(function (s) {
    return {
      type: 'Feature',
      properties: {
        id: s.id, who: s.who, prio: s.prio, msg: s.msg,
        needs: s.needs.join(', '), hops: s.hops, ttl: s.ttl,
        batt: s.batt, age: s.age
      },
      geometry: { type: 'Point', coordinates: [s.lon, s.lat] }
    };
  });

  /* ── aid posts ───────────────────────────────────────── */
  const AID = [
    { kind:'Rescue command',    note:'Team dispatch and mesh uplink',  lon:150.6931, lat:-33.7511 },
    { kind:'Evacuation centre', note:'Registration, ~400 capacity',    lon:150.8088, lat:-33.6121 },
    { kind:'Medical camp',      note:'Triage, 24 beds',                lon:150.9245, lat:-33.9166 },
    { kind:'Water point',       note:'Treated water, refill only',     lon:150.7011, lat:-34.0512 },
    { kind:'Shelter',           note:'School hall, ~180 capacity',     lon:151.0043, lat:-33.8168 }
  ];
  const aidFeatures = AID.map(function (a) {
    return { type:'Feature', properties:{ kind:a.kind, note:a.note }, geometry:{ type:'Point', coordinates:[a.lon,a.lat] } };
  });

  const fc = function (f) { return { type:'FeatureCollection', features:f }; };

  return {
    center: CENTER,
    signals: SIGNALS,
    heat:    fc(heatFeatures),
    safe:    fc(safeFeatures),
    sos:     fc(sosFeatures),
    aid:     fc(aidFeatures),
    totalNodes: heatFeatures.length
  };
})();
