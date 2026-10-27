
(() => {
  const oa = window.__oa;
  if (!oa) return JSON.stringify({error: 'helper_missing'});
  const el = oa.find(__OA_ID__);
  if (!el) return JSON.stringify({error: 'not_found'});
  el.scrollIntoView({block: 'center'});
  const rect = el.getBoundingClientRect();
  return JSON.stringify({ok: true, x: rect.left + rect.width / 2, y: rect.top + rect.height / 2});
})()
