
(() => {
  if (window.__oa && window.__oa.version === 2) return true;
  const norm = (s) => String(s == null ? '' : s).toLowerCase()
    .replace(/[\s_\-*:：·•/\\（）()【】\[\]<>]/g, '');
  const textOf = (node) => {
    if (!node) return '';
    const t = node.innerText != null ? node.innerText : node.textContent;
    return String(t || '').replace(/\s+/g, ' ').trim();
  };
  const visible = (el) => {
    if (!el || !el.isConnected) return false;
    const style = window.getComputedStyle(el);
    if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0') return false;
    const r = el.getBoundingClientRect();
    return r.width > 1 && r.height > 1;
  };
  const visibleChoice = (el) => {
    if (!el) return false;
    if (visible(el)) return true;
    if (el.labels) {
      for (const label of el.labels) if (visible(label)) return true;
    }
    const wrap = el.closest('label');
    return !!(wrap && visible(wrap));
  };
  const ITEM_SELECTOR = [
    '.ant-form-item', '.el-form-item', '.form-item', '.form-group', '.field', '.weui-cell', 'tr', 'li',
    '.form-row', '.input-wrap', '.form-line', '.ant-row', '.control-group',
    '[class*="form-item" i]', '[class*="formItem"]', '[class*="form-item"]',
    '[class*="form-field" i]', '[class*="formField"]', '[class*="field-wrapper" i]', '[class*="fieldWrapper"]',
    '[class*="apply-field"]', '[class*="applyField"]', '[class*="field-container" i]', '[class*="fieldContainer"]',
  ].join(',');
  const ITEM_LABEL_SELECTOR = [
    'label', '.label', '.form-label', '.ant-form-item-label', '.el-form-item__label', 'th',
    '.title', '[class*="field-label" i]', '[class*="fieldLabel"]',
    '[class*="form-label" i]', '[class*="formLabel"]', '[class*="control-label" i]', '[class*="controlLabel"]',
  ].join(',');
  const itemOf = (el) => {
    try { return el.closest(ITEM_SELECTOR); } catch (e) { return null; }
  };
  const isRequired = (el) => {
    if (el.required || el.getAttribute('aria-required') === 'true') return true;
    if (/required/i.test(String(el.className || ''))) return true;
    const item = itemOf(el);
    if (item) {
      if (/required/i.test(String(item.className || ''))) return true;
      if (item.querySelector('[class*="required" i], [aria-required="true"]')) return true;
      const itemLabel = item.querySelector(ITEM_LABEL_SELECTOR);
      if (itemLabel && /^\s*[*＊]/.test(textOf(itemLabel))) return true;
    }
    if (el.labels) {
      for (const label of el.labels) {
        if (textOf(label).indexOf('*') >= 0 || /required/i.test(String(label.className || ''))) return true;
      }
    }
    return false;
  };
  const allRoots = () => {
    const roots = [];
    const seenRoots = new Set();
    const seenDocs = new Set();
    const addRoot = (root, doc) => {
      if (!root || seenRoots.has(root)) return;
      seenRoots.add(root);
      roots.push({root, doc});
    };
    const scanRoot = (root, doc) => {
      let nodes = [];
      try { nodes = Array.from(root.querySelectorAll('*')); } catch (e) { return; }
      if (nodes.length > 20000) nodes = nodes.slice(0, 20000);
      for (const el of nodes) {
        if (el.shadowRoot) {
          addRoot(el.shadowRoot, doc);
          scanRoot(el.shadowRoot, doc);
        }
        if (el.tagName === 'IFRAME' || el.tagName === 'FRAME') {
          let childDoc = null;
          try { childDoc = el.contentDocument; } catch (e) { childDoc = null; }
          if (childDoc && !seenDocs.has(childDoc)) {
            seenDocs.add(childDoc);
            addRoot(childDoc, childDoc);
            scanRoot(childDoc, childDoc);
          }
        }
      }
    };
    seenDocs.add(document);
    addRoot(document, document);
    scanRoot(document, document);
    return roots;
  };
  const find = (id) => {
    const needle = '[data-oa-id="' + String(id).replace(/"/g, '') + '"]';
    for (const {root} of allRoots()) {
      let found = null;
      try { found = root.querySelector(needle); } catch (e) { found = null; }
      if (found) return found;
    }
    return null;
  };
  const label = (el) => {
    const parts = [];
    const push = (value) => {
      const text = String(value || '').replace(/\s+/g, ' ').trim();
      if (text) parts.push(text);
    };
    if (el.labels) {
      for (const item of el.labels) push(textOf(item));
    }
    push(el.getAttribute('aria-label'));
    const labelledby = el.getAttribute('aria-labelledby');
    if (labelledby) {
      const root = el.getRootNode && el.getRootNode();
      for (const id of labelledby.split(/\s+/)) {
        let node = null;
        try { node = (root && root.getElementById) ? root.getElementById(id) : document.getElementById(id); } catch (e) { node = null; }
        if (node) push(textOf(node));
      }
    }
    push(el.getAttribute('placeholder'));
    push(el.getAttribute('title'));
    push(el.getAttribute('data-label'));
    const wrapped = el.closest('label');
    if (wrapped) push(textOf(wrapped));
    const item = itemOf(el);
    if (item) {
      const itemLabel = item.querySelector(ITEM_LABEL_SELECTOR);
      if (itemLabel) {
        const labelText = textOf(itemLabel);
        const sameAsWrapped = el.labels && Array.from(el.labels).some((node) => textOf(node) === labelText);
        if (labelText && !sameAsWrapped) push(labelText);
      } else {
        const clone = item.cloneNode(true);
        clone.querySelectorAll('input,textarea,select,button,option,svg,img').forEach((node) => node.remove());
        const text = textOf(clone);
        if (text) push(text.slice(0, 120));
      }
    }
    if (!parts.length) {
      let prev = el.previousElementSibling;
      let guard = 0;
      while (prev && guard++ < 4) {
        const text = textOf(prev);
        if (text && text.length < 120) { push(text); break; }
        prev = prev.previousElementSibling;
      }
    }
    if (!parts.length) {
      push(el.getAttribute('name'));
      push(el.id);
    }
    const seen = new Set();
    return parts.filter((text) => {
      if (seen.has(text)) return false;
      seen.add(text);
      return true;
    }).join(' | ').slice(0, 200);
  };
  const groupLabel = (el) => {
    const fieldset = el.closest('fieldset');
    if (fieldset) {
      const legend = fieldset.querySelector('legend');
      if (legend) return textOf(legend);
    }
    const item = itemOf(el);
    if (item) {
      const itemLabel = item.querySelector(ITEM_LABEL_SELECTOR);
      if (itemLabel) return textOf(itemLabel).slice(0, 120);
      const clone = item.cloneNode(true);
      clone.querySelectorAll('input,textarea,select,button,svg,img').forEach((node) => node.remove());
      const text = textOf(clone);
      if (text) return text.slice(0, 100);
    }
    return '';
  };
  const context = (el) => {
    let node = el;
    for (let depth = 0; node && depth < 8; depth++) {
      const container = node.closest && node.closest('fieldset, section, .form-section, .section, .ant-card, .el-card, .card, .panel, .step-pane, [class*="form-block"], [class*="section"]');
      if (container) {
        const heading = container.querySelector('legend, h1, h2, h3, h4, h5, h6, .title, .section-title, .ant-card-head-title, .el-card__header');
        if (heading) return textOf(heading).slice(0, 100);
        const text = textOf(container);
        if (text) return text.slice(0, 100);
      }
      let prev = node.previousElementSibling;
      while (prev) {
        if (/^H[1-6]$/.test(prev.tagName) || prev.matches('.title,.section-title,.card-title')) {
          return textOf(prev).slice(0, 100);
        }
        prev = prev.previousElementSibling;
      }
      node = node.parentElement;
    }
    return '';
  };
  const DIALOG_SELECTOR = [
    '[role="dialog"]', '.ant-modal', '.el-dialog', '.modal', '.layui-layer', '.weui-dialog', '.confirm-dialog',
    '[class*="modal" i]', '[class*="dialog" i]', '[class*="popup" i]',
  ].join(',');
  const dialogOf = (el) => {
    try { return el.closest(DIALOG_SELECTOR); } catch (e) { return null; }
  };
  const dialogText = (el) => {
    const dialog = dialogOf(el);
    return dialog ? textOf(dialog).slice(0, 400) : '';
  };
  const dialogLogin = (el) => {
    const text = dialogText(el);
    if (!text) return false;
    return /登录|注册|验证码|密码|sign in|log in|login|create account/i.test(text);
  };
  const sectionNodes = (root) => {
    const selectors = [
      'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'legend',
      '[class*="blockTitle"]', '[class*="block-title"]', '[class*="section-title" i]', '[class*="sectionTitle"]',
      '[class*="form-section" i]', '[class*="formSection"]', '[class*="card-title" i]', '[class*="cardTitle"]',
      '[class*="panel-title" i]', '[class*="panelTitle"]', '[class*="group-title" i]', '[class*="groupTitle"]',
      '[class*="step-title" i]', '[class*="stepTitle"]', '[class*="part-title" i]', '[class*="partTitle"]',
      '[class*="modal-header" i]', '[class*="modalHeader"]',
    ].join(',');
    const result = [];
    const seen = new Set();
    let nodes = [];
    try { nodes = root.querySelectorAll(selectors); } catch (e) { nodes = []; }
    for (const node of nodes) {
      if (!visible(node)) continue;
      const text = textOf(node);
      if (!text || text.length > 80) continue;
      if (seen.has(text)) continue;
      seen.add(text);
      result.push({node, text});
    }
    return result;
  };
  const sectionOf = (el, sections) => {
    let best = '';
    for (const item of sections || []) {
      const position = item.node.compareDocumentPosition(el);
      if (position & Node.DOCUMENT_POSITION_FOLLOWING) {
        best = item.text;
      } else if (position & Node.DOCUMENT_POSITION_PRECEDING) {
        break;
      }
    }
    return best || context(el);
  };
  const isSearchable = (el, custom) => {
    if (!custom || el.tagName.toLowerCase() !== 'input') return false;
    if (el.readOnly) return false;
    const placeholder = String(el.getAttribute('placeholder') || '');
    const cls = String(el.className || '');
    return /请输入|搜索|search|input/i.test(placeholder) || /search/i.test(cls);
  };
  window.__oa = {
    version: 2, norm, textOf, visible, visibleChoice, isRequired, allRoots, find, label, groupLabel,
    context, itemOf, dialogOf, dialogText, dialogLogin, sectionNodes, sectionOf, isSearchable,
  };
  return true;
})()
