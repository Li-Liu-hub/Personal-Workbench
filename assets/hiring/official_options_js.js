
(() => {
  const oa = window.__oa;
  if (!oa) return JSON.stringify({error: 'helper_missing'});
  const wanted = oa.norm(__VALUE__);
  const selectors = [
    '[role="option"]', '[role="treeitem"]',
    '.ant-select-item-option', '.el-select-dropdown__item', '.el-cascader-node',
    '.option', '.dropdown-item', '.select-item', 'li[data-value]',
    '[class*="option" i]', '[class*="Option"]',
    '[class*="dropdown" i] li', '[class*="Dropdown"] li',
    '[class*="menu-item" i]', '[class*="menuItem"]',
  ].join(',');
  const items = [];
  for (const {root} of oa.allRoots()) {
    let nodes = [];
    try { nodes = root.querySelectorAll(selectors); } catch (e) { nodes = []; }
    for (const node of nodes) {
      if (!oa.visible(node)) continue;
      const text = oa.textOf(node);
      if (!text) continue;
      items.push({text, node});
    }
  }
  let best = null;
  for (const item of items) {
    const text = oa.norm(item.text);
    if (text === wanted) { best = item; break; }
    if (!best && (text.indexOf(wanted) >= 0 || wanted.indexOf(text) >= 0)) best = item;
  }
  if (!best) {
    return JSON.stringify({ok: false, error: 'option_not_found', options: items.slice(0, 20).map((item) => item.text)});
  }
  best.node.scrollIntoView({block: 'nearest'});
  const rect = best.node.getBoundingClientRect();
  return JSON.stringify({ok: true, text: best.text, x: rect.left + rect.width / 2, y: rect.top + rect.height / 2});
})()
