/* ════════════════════════════════════════════════════════════
   ReUnite — nav, scroll spy, reveal
   ════════════════════════════════════════════════════════════ */
(function () {
  'use strict';

  const nav = document.getElementById('nav');
  const burger = document.getElementById('burger');
  const mobile = document.getElementById('mobilenav');

  /* sticky shadow */
  const onScroll = function () { nav.classList.toggle('is-stuck', window.scrollY > 8); };
  window.addEventListener('scroll', onScroll, { passive: true });
  onScroll();

  /* mobile menu */
  if (burger && mobile) {
    burger.addEventListener('click', function () {
      const open = burger.getAttribute('aria-expanded') !== 'true';
      burger.setAttribute('aria-expanded', String(open));
      burger.setAttribute('aria-label', open ? 'Close menu' : 'Open menu');
      nav.classList.toggle('is-open', open);
      mobile.hidden = !open;
    });
    mobile.addEventListener('click', function (e) {
      if (e.target.tagName === 'A') {
        burger.setAttribute('aria-expanded', 'false');
        burger.setAttribute('aria-label', 'Open menu');
        nav.classList.remove('is-open');
        mobile.hidden = true;
      }
    });
    document.addEventListener('keydown', function (e) {
      if (e.key === 'Escape' && !mobile.hidden) burger.click();
    });
  }

  /* reveal on scroll */
  const reveals = document.querySelectorAll('.reveal');
  if ('IntersectionObserver' in window) {
    const io = new IntersectionObserver(function (entries) {
      entries.forEach(function (en) {
        if (en.isIntersecting) { en.target.classList.add('is-in'); io.unobserve(en.target); }
      });
    }, { rootMargin: '0px 0px -8% 0px', threshold: 0.08 });
    reveals.forEach(function (el) { io.observe(el); });
  } else {
    reveals.forEach(function (el) { el.classList.add('is-in'); });
  }

  /* Jumping straight to an anchor (#map from the hero CTA, or a shared link)
     skips past everything above it, so sweep those in directly. */
  function sweepAbove() {
    const cut = window.innerHeight * 0.92;
    reveals.forEach(function (el) {
      if (el.getBoundingClientRect().top < cut) el.classList.add('is-in');
    });
  }
  window.addEventListener('hashchange', function () { setTimeout(sweepAbove, 60); });
  window.addEventListener('load', function () { setTimeout(sweepAbove, 60); });
  if (location.hash) setTimeout(sweepAbove, 60);

  /* scroll spy on the primary nav */
  const links = Array.prototype.slice.call(document.querySelectorAll('.nav__links a'));
  const sections = links
    .map(function (a) { return document.querySelector(a.getAttribute('href')); })
    .filter(Boolean);

  if (sections.length && 'IntersectionObserver' in window) {
    const spy = new IntersectionObserver(function (entries) {
      entries.forEach(function (en) {
        if (!en.isIntersecting) return;
        links.forEach(function (a) {
          a.classList.toggle('is-active', a.getAttribute('href') === '#' + en.target.id);
        });
      });
    }, { rootMargin: '-45% 0px -50% 0px' });
    sections.forEach(function (s) { spy.observe(s); });
  }
})();
