/* Frosted Glass — 可选极轻视差：鼠标位置写入 --glass-px/--glass-py，只驱动
   body::before 装饰层。prefers-reduced-motion: reduce 时完全不启用；
   启动前先清理上一实例；本 <script> 被移出 DOM 后自行停止。 */
(function () {
  if (window.__sakuraGlassFx) { try { window.__sakuraGlassFx(); } catch (e) {} }

  var root = document.documentElement;
  try { if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return; } catch (e) {}

  var el = document.currentScript || document.querySelector('script[data-theme-js]');
  var stopped = false, queued = false, px = 0, py = 0;

  function flush() {
    queued = false;
    if (stopped) return;
    root.style.setProperty('--glass-px', px.toFixed(3));
    root.style.setProperty('--glass-py', py.toFixed(3));
  }

  function onMove(e) {
    if (stopped) return;
    px = (e.clientX / window.innerWidth - 0.5) * 2;
    py = (e.clientY / window.innerHeight - 0.5) * 2;
    if (!queued) { queued = true; requestAnimationFrame(flush); }
  }

  function stop() {
    if (stopped) return;
    stopped = true;
    window.removeEventListener('mousemove', onMove, false);
    clearInterval(watch);
    // 只有仍是当前实例时才清变量，避免误伤刚装上的新实例
    if (window.__sakuraGlassFx === stop) {
      root.style.removeProperty('--glass-px');
      root.style.removeProperty('--glass-py');
      window.__sakuraGlassFx = null;
    }
  }

  window.addEventListener('mousemove', onMove, { passive: true });

  // 低频存活检查：主题切换会把本脚本节点移除
  var watch = setInterval(function () {
    if (!el || !document.contains(el)) stop();
  }, 1500);

  window.__sakuraGlassFx = stop;
})();
