// SPDX-License-Identifier: GPL-3.0-or-later
// Same-origin presentation only. Device access remains inside the static client.
(() => {
  const linux = /Linux/i.test(navigator.userAgent) && !/Android|CrOS/i.test(navigator.userAgent);
  for (const notice of document.querySelectorAll("[data-byakko-linux-requirements]")) notice.hidden = !linux;
  for (const frame of document.querySelectorAll("iframe[data-byakko-configurator]")) {
    let dispose;
    function attach() {
      dispose?.();
      const doc = frame.contentDocument;
      if (!doc?.body) return; // Keep the ordinary iframe/new-tab fallback.
      const root = doc.documentElement;
      root.style.setProperty("--embed-font", getComputedStyle(document.body).fontFamily);
      let pending = false;
      function measure() {
        pending = false;
        // Body's own box can shrink; document.scrollHeight includes the old viewport.
        const height = Math.ceil(doc.body.getBoundingClientRect().height);
        if (height > 0 && frame.style.height !== `${height}px`) frame.style.height = `${height}px`;
        const rect = frame.getBoundingClientRect();
        const top = Math.max(0, rect.top), bottom = Math.min(innerHeight, rect.bottom);
        const visible = Math.max(160, bottom - top);
        root.style.setProperty("--embed-visible-height", `${visible}px`);
        root.style.setProperty("--embed-dialog-center", `${top - rect.top + visible / 2}px`);
      }
      function schedule() {
        if (!pending) { pending = true; requestAnimationFrame(measure); }
      }
      const observer = new ResizeObserver(schedule);
      observer.observe(doc.body);
      addEventListener("resize", schedule);
      addEventListener("scroll", schedule, { passive: true });
      dispose = () => {
        observer.disconnect();
        removeEventListener("resize", schedule);
        removeEventListener("scroll", schedule);
      };
      measure();
    }
    frame.addEventListener("load", attach);
    if (frame.contentDocument?.readyState === "complete") attach();
  }
})();
