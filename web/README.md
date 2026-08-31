# ReUnite — marketing site

Static site for the ReUnite mesh project. Lives alongside the Rust core in
`crates/` and the Flutter app in `mobile/`; it shares no build tooling with
them — open `index.html` or serve this folder.

## Run locally

```bash
cd web && python3 -m http.server 4321
```

Then open <http://localhost:4321>. Any static server works — there is no
build step.

## Structure

```
index.html              landing page, all sections
map.html                full-screen signal map, log in a slide-out drawer
assets/css/styles.css   design tokens + every component
assets/js/config.js     Google Maps API key + async loader + failure gate
assets/js/mapstyle.js   Google Maps JSON style (bright, low-saturation)
assets/js/heatlayer.js  canvas heatmap overlay (replaces the discontinued
                        google.maps.visualization.HeatmapLayer)
assets/js/data.js       simulated Sydney scenario — 415 nodes, 16 SOS
                        signals, 7 hazards, 5 aid posts, seeded PRNG
assets/js/globe.js      rotating Earth backdrop for the landing hero
assets/js/map.js        live SOS map: heatmap, markers, filters, signal log
assets/js/main.js       nav, scroll spy, reveal-on-scroll (index.html)
assets/js/drawer.js     signal-log drawer (map.html)
assets/js/theme.js      light/dark flag, toggle wiring, localStorage
assets/js/pulselayer.js pulsing beacon rings under the victim pins
```

`map.js` drives both pages. It looks for the same ids on each
(`#mapCanvas`, `#sigList`, `#sigCount`, `#mapFail`, `[data-layer]`,
`[data-prio]`, `#mapReset`), so the map feature has one implementation,
not two. The fit padding measures whatever overlays are present, so the
floating control card on `map.html` never hides a cluster.

## Setup after cloning

`assets/js/config.js` is **not** in the repo — it holds the Google Maps API
key. Create it before running:

```bash
cp assets/js/config.example.js assets/js/config.js
# then paste your key into googleMapsKey
```

Without it the map shows its fallback message and the signal log still
works; nothing else on the page is affected.

If you deploy from this repo (e.g. Netlify), `node build.js` runs automatically during build. Set `YOUR_GOOGLE_MAPS_API_KEY` (or `GOOGLE_MAPS_API_KEY`) as an Environment Variable in your deployment settings and `build.js` will inject it into `assets/js/config.js`.

## Google Maps key — read this before deploying

`assets/js/config.js` holds the Maps JavaScript API key. A browser-side map
key is **always** visible to anyone who opens the page; that is unavoidable.
The protection is a restriction, not secrecy:

1. Google Cloud Console → **APIs & Services → Credentials** → select the key
2. **Application restrictions** → HTTP referrers:
   - `https://your-domain.com/*`
   - `http://localhost:4321/*`
3. **API restrictions** → Maps JavaScript API only
4. Enable **billing** on the project, or Maps renders a "development purposes
   only" watermark

The key currently in `config.js` has been shared in plain text. Rotate it in
the console and paste the replacement before going live.

If the key is rejected, `gm_authFailure` fires: the live map shows a fallback
message and the hero falls back to a schematic grid, so the page stays usable.

## The data is simulated

Every signal, hazard and aid post in `data.js` is generated for
demonstration. The scenario is a Hawkesbury–Nepean flood across Greater
Sydney. It is **not** real emergency data and must not be used for real
decisions. The page says so in the map badge and the footer.

## Theme

Light by default, with a toggle in the nav (and in the mobile menu) and in
the map page's app bar. The choice is stored in `localStorage`; a tiny inline
script in each page's `<head>` applies it before first paint so the wrong
theme never flashes.

One attribute drives everything:

```html
<html lang="en" data-theme="dark">
```

`ReUniteTheme.set('dark')` / `.toggle()` do it at runtime and fire a
`reunite:theme` event. Listening to it, `map.js` re-reads the palette and
swaps the Google Maps style, the pin icons and the heat ramp in place;
`globe.js` swaps its own palette. `theme.js` must load before the other
modules, since they read the flag through `ReUniteTheme.pick(light, dark)`
and `ReUniteTheme.css(name)`.

## Map model

The map answers one question — *where are the people who need help* — so it
carries three layers and nothing else:

| Layer | Mark |
|---|---|
| SOS density | canvas heat overlay |
| SOS signals | red teardrop pins with pulsing beacon rings |
| Aid posts | small green squares |

Every signal is the same red. There are no priority tiers and no hazard
layer; both were removed from the map, along with the priority filter and
the per-tag colours in the signal log.

The `--prio-*` tokens still exist in `styles.css` because the landing page's
Feature 01 card illustrates the app's priority-broadcast capability. Nothing
on the map reads them.

### Organic density

`heatlayer.js` does not draw one radial gradient per point — that renders a
perfect circle, which reads as a graphic rather than a spreading field.
Each point is drawn as four offset, unequally-sized lobes whose positions
are hashed from the point index, so blobs grow irregular edges and merge
into each other. Hashing (rather than `Math.random`) keeps the shape stable
across re-renders instead of shimmering on every pan.

Each cluster is built from 3–5 sub-lobes whose angles are spread evenly
around the pin with jitter, and each lobe is offset, rotated and stretched
differently. A single isotropic Gaussian gives a perfectly round footprint;
this gives an irregular one that still stays centred on the marker. Letting
the lobe offsets float freely instead pulls the whole mass off the pin —
invisible at metro zoom, glaring at street level.

The field is generated from the pin locations: nine tightly-packed core
points under each marker plus the handset cluster that relayed it, so the
density always peaks red exactly where a victim is.

### Hotspots stay pinned

The overlay repaints only when the map settles. Between repaints the
painted image is re-pegged to its anchor **and scaled** by
`2^(zoom - paintZoom)` about that anchor. Moving a canvas painted at the old
scale — without scaling it — is what makes heat slide off the markers during
a zoom. With the transform in place a hotspot stays locked to its
coordinates throughout the gesture, then repaints crisply on idle.

## Accessibility notes

- The **signal log** is the accessible equivalent of the map: every plotted
  value as keyboard-navigable buttons with descriptive labels
- Hazards are diamonds, aid posts are squares — pin type never depends on
  colour alone
- `prefers-reduced-motion` stops the ticker, the relay animation, the beacon
  pulse and every scroll reveal
- Skip link, visible focus rings, 44px minimum touch targets

## Earth imagery

Two NASA textures ship in `assets/img/`, both public domain, both
equirectangular (2048×1024):

| File | Source | Used |
|---|---|---|
| `earth.jpg` | Blue Marble, daylight with clouds | **current** |
| `earth-night.jpg` | Black Marble 2012, Earth at Night | alternative |

Switching between them is the one `img.src` line in `loadPhoto()`, plus the
exposure constants that go with it: `photoGain` per theme, the limb term in
`renderSphere()`, the light-mode lift in `loadPhoto()`, and `rim`/`grat`.
Black Marble is far darker than Blue Marble, so swapping the file alone
leaves one of the two badly exposed.

`globe.js` samples the texture through an inverse orthographic projection
to build the rotating hemisphere. If the file is missing or the request is
blocked, `buildTexture()` paints a procedural Earth instead and the hero
still renders — nothing hard-depends on the image.

Note that an orthographic *render* of a globe — a photo of Earth taken from
space — cannot be used here: wrapping one onto a sphere projects it twice
and smears the edges. The texture has to be a flat lat/lon map at 2:1.

## The globe mesh

Nodes are sampled on land from the continent polygons and thinned evenly.
The spiral used for sampling walks pole to pole, so collecting land hits
until a cap is reached puts every node in the far north — the full sphere
is sampled first, then subsampled.

Relays pick a partner by great-circle distance (≈34°–106° apart) and the
arc is drawn with spherical interpolation. Linear lon/lat interpolation is
fine for a short hop but visibly leaves the sphere once the span is large.
`PULSE_MS` and `SPAWN_MS` in `globe.js` control how long each relay lives
and how often one fires — together they set how packed the animation looks.
