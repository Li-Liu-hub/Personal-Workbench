
(() => {
  const oa = window.__oa;
  if (!oa) return false;
  const el = oa.find(__OA_ID__);
  if (!el) return false;
  el.dispatchEvent(new Event('input', {bubbles: true}));
  el.dispatchEvent(new Event('change', {bubbles: true}));
  return true;
})()
