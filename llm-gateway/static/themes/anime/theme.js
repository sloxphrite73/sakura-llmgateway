/* Anime (二次元) — falling sakura decoration for the console.
   Loaded by the theme runtime as <script src="/themes/anime/theme.js"> after
   theme.css, appended to <body>. Plain ES5, no network, no eval, no dependency.

   Contract implemented here:
   1. the first statement kills a previously registered instance;
   2. the destroy function is published as window.__sakuraAnimeFx and removes
      every node this file created (plus its rAF / timer / listener);
   3. a low-frequency (1500 ms) watchdog calls destroy() once this <script> node
      is no longer in the document (theme switched away / re-rendered);
   4. with prefers-reduced-motion: reduce nothing at all is created.
   The animation itself runs on the compositor (CSS keyframes on transform
   only); this script touches the DOM once, never per frame. */

(function () {
  /* 1) Clean up the previous instance — must stay the first statement. */
  if (window.__sakuraAnimeFx) {
    window.__sakuraAnimeFx();
    window.__sakuraAnimeFx = null;
  }

  /* 4) Reduced motion: create nothing at all (no layer, no timer, no rAF). */
  var mql = window.matchMedia ? window.matchMedia('(prefers-reduced-motion: reduce)') : null;
  if (mql && mql.matches) return;

  /* ---- own <script> element (for the self-stop watchdog) ---- */
  var scriptEl = document.currentScript || null;
  if (!scriptEl) {
    var list = document.querySelectorAll('script[data-theme-js]');
    for (var i = list.length - 1; i >= 0; i--) {
      var src = list[i].src || '';
      if (src.indexOf('theme.js') !== -1) { scriptEl = list[i]; break; }
    }
  }

  var host = document.body || document.documentElement;
  if (!host) return;

  /* ---- tuning: petal count stays inside 18–28 in total ---- */
  var CONFIG = {
    back:  { count: 16, size: [10, 16], opacity: [0.50, 0.70], duration: [16, 26], spin: [5, 9] },
    front: { count: 6,  size: [8, 12],  opacity: [0.35, 0.50], duration: [20, 30], spin: [6, 11] }
  };

  /* blush / lilac petal tones — [inner, outer] of the gradient */
  var TONES = [
    ['#ffe6ef', '#ff9dc4'],
    ['#fff1f7', '#ffb3d2'],
    ['#f6e6ff', '#d9a9f2'],
    ['#fff6fa', '#ffc2db']
  ];

  var layers = [];
  var rafId = 0;
  var pollId = 0;
  var destroyed = false;

  function rnd(min, max) { return min + Math.random() * (max - min); }

  function buildLayer(spec, extraClass) {
    var layer = document.createElement('div');
    layer.className = 'anime-fx ' + extraClass;
    layer.setAttribute('aria-hidden', 'true');

    var frag = document.createDocumentFragment();
    for (var n = 0; n < spec.count; n++) {
      var petal = document.createElement('i');
      petal.className = 'anime-petal';

      var size = rnd(spec.size[0], spec.size[1]);
      var dur = rnd(spec.duration[0], spec.duration[1]);
      var tone = TONES[Math.floor(Math.random() * TONES.length)];
      var st = petal.style;

      st.setProperty('--anime-x', rnd(1, 99).toFixed(2) + '%');
      st.setProperty('--anime-size', size.toFixed(1) + 'px');
      st.setProperty('--anime-dur', dur.toFixed(2) + 's');
      /* negative delay = petals start already spread over the viewport */
      st.setProperty('--anime-delay', (-rnd(0, dur)).toFixed(2) + 's');
      st.setProperty('--anime-spin', rnd(spec.spin[0], spec.spin[1]).toFixed(2) + 's');
      st.setProperty('--anime-spin-dir', Math.random() < 0.5 ? 'normal' : 'reverse');
      st.setProperty('--anime-op', rnd(spec.opacity[0], spec.opacity[1]).toFixed(2));
      st.setProperty('--anime-c1', tone[0]);
      st.setProperty('--anime-c2', tone[1]);

      frag.appendChild(petal);
    }

    layer.appendChild(frag);
    host.appendChild(layer);
    layers.push(layer);
  }

  function reveal() {
    for (var n = 0; n < layers.length; n++) {
      if (layers[n].classList) layers[n].classList.add('is-live');
      else layers[n].className += ' is-live';
    }
  }

  buildLayer(CONFIG.back, 'anime-fx-back');
  buildLayer(CONFIG.front, 'anime-fx-front');

  /* one-shot rAF: fade the layer in on the next paint (no per-frame work) */
  if (window.requestAnimationFrame) {
    rafId = window.requestAnimationFrame(function () {
      rafId = 0;
      if (!destroyed) reveal();
    });
  } else {
    reveal();
  }

  function onMotionChange() {
    if (mql && mql.matches) destroy();
  }

  /* 2) destroy: cancel rAF + watchdog, drop the listener, remove our nodes. */
  function destroy() {
    if (destroyed) return;
    destroyed = true;

    if (rafId) { window.cancelAnimationFrame(rafId); rafId = 0; }
    if (pollId) { window.clearInterval(pollId); pollId = 0; }

    if (mql && mql.removeEventListener) mql.removeEventListener('change', onMotionChange);

    for (var n = 0; n < layers.length; n++) {
      var layer = layers[n];
      if (layer && layer.parentNode) layer.parentNode.removeChild(layer);
    }
    layers = [];

    /* only unregister if we are still the current instance */
    if (window.__sakuraAnimeFx === destroy) window.__sakuraAnimeFx = null;
  }

  window.__sakuraAnimeFx = destroy;

  if (mql && mql.addEventListener) mql.addEventListener('change', onMotionChange);

  /* 3) self-stop: the theme runtime removes our <script> node on theme switch. */
  if (scriptEl) {
    pollId = window.setInterval(function () {
      if (!document.contains(scriptEl)) destroy();
    }, 1500);
  }
})();
