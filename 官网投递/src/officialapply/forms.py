"""Browser-side page inspection and generic form filling."""

from __future__ import annotations

import json
import re
import time
from pathlib import Path
from typing import Any

from officialapply.browser import CDPError, CDPTab
from officialapply.errors import ToolError
from officialapply.profile import mask_email, mask_id, mask_phone, normalize_label

HELPER_JS = r"""
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
"""

INSPECT_JS = r"""
(() => {
  const oa = window.__oa;
  if (!oa) return JSON.stringify({error: 'helper_missing'});
  const roots = oa.allRoots();
  const sectionsByRoot = roots.map(({root}) => oa.sectionNodes(root));
  const prefix = 'oa' + Date.now().toString(36);
  let seq = 0;
  let order = 0;
  const assign = (el) => {
    const id = prefix + '-' + (seq++);
    el.setAttribute('data-oa-id', id);
    return id;
  };
  const fields = [];
  const buttons = [];
  const links = [];
  const consent = [];
  const seenElements = new Set();
  const seenRadio = new Set();
  const MAX_FIELDS = 300;
  const fieldSelectors = 'input, textarea, select, [contenteditable="true"], [contenteditable=""], [role="combobox"], [aria-haspopup="listbox"]';
  for (let rootIndex = 0; rootIndex < roots.length; rootIndex++) {
    const root = roots[rootIndex].root;
    let nodes = [];
    try { nodes = root.querySelectorAll(fieldSelectors); } catch (e) { nodes = []; }
    for (const el of nodes) {
      if (fields.length >= MAX_FIELDS) break;
      if (seenElements.has(el)) continue;
      seenElements.add(el);
      const tag = el.tagName.toLowerCase();
      const type = (el.getAttribute('type') || '').toLowerCase();
      if (tag === 'button' || tag === 'a') continue;
      if (tag === 'input' && ['hidden', 'submit', 'button', 'reset', 'image'].indexOf(type) >= 0) continue;
      const isFile = tag === 'input' && type === 'file';
      if (!isFile && !oa.visibleChoice(el)) continue;
      if (type === 'radio') {
        const name = el.getAttribute('name') || '';
        const key = name ? ('r:' + rootIndex + ':' + name) : '';
        if (key && seenRadio.has(key)) continue;
        if (key) seenRadio.add(key);
        let group = [el];
        if (name) {
          try {
            group = Array.from(root.querySelectorAll('input[type=radio]')).filter((item) => (item.getAttribute('name') || '') === name);
          } catch (e) { group = [el]; }
        }
        const options = group.map((item) => ({
          value: item.value || '',
          label: oa.label(item),
          checked: !!item.checked,
          disabled: !!item.disabled,
        }));
        const id = assign(el);
        const sectionText = oa.sectionOf(el, sectionsByRoot[rootIndex]) || '';
        fields.push({
          id, kind: 'radio', tag, input_type: 'radio', name,
          label: oa.groupLabel(el) || oa.label(el),
          context: sectionText, section: sectionText,
          group_label: oa.groupLabel(el),
          placeholder: '', aria_label: el.getAttribute('aria-label') || '',
          required: oa.isRequired(el),
          disabled: !!el.disabled, readonly: false,
          value: (group.find((item) => item.checked) || {}).value || '',
          checked: group.some((item) => item.checked),
          options, accept: '', multiple: false, max_length: 0,
          custom_select: false, searchable: false, hidden: !oa.visibleChoice(el),
          in_dialog: !!oa.dialogOf(el), dialog_login: oa.dialogLogin(el),
          order: order++,
          in_iframe: el.ownerDocument !== document,
          in_shadow: !!(el.getRootNode && el.getRootNode() !== el.ownerDocument),
        });
        continue;
      }
      const id = assign(el);
      let kind = 'input';
      if (tag === 'select') kind = 'select';
      else if (tag === 'textarea') kind = 'textarea';
      else if (type === 'checkbox') kind = 'checkbox';
      else if (el.isContentEditable) kind = 'contenteditable';
      else if (isFile) kind = 'file';
      const options = tag === 'select' ? Array.from(el.options).map((option) => ({
        value: option.value,
        label: oa.textOf(option),
        disabled: !!option.disabled,
        selected: !!option.selected,
      })) : [];
      const cls = String(el.className || '');
      const custom = tag !== 'select' && !isFile && (
        el.getAttribute('role') === 'combobox' ||
        el.getAttribute('aria-haspopup') === 'listbox' ||
        /(?:^|[\s_-])(select|dropdown|cascader)(?:[\s_-]|$)/i.test(cls)
      );
      const labelText = oa.label(el);
      const customValue = el.value || el.getAttribute('data-value') || oa.textOf(el);
      const sectionText = oa.sectionOf(el, sectionsByRoot[rootIndex]) || '';
      const groupLabelText = oa.groupLabel(el);
      fields.push({
        id, kind, tag, input_type: type || (tag === 'textarea' ? 'textarea' : ''),
        name: el.getAttribute('name') || '',
        label: labelText, context: sectionText, section: sectionText,
        group_label: groupLabelText,
        placeholder: el.getAttribute('placeholder') || '',
        aria_label: el.getAttribute('aria-label') || '',
        required: oa.isRequired(el),
        disabled: !!el.disabled, readonly: !!el.readOnly,
        value: tag === 'select' ? el.value : (custom ? customValue : (el.isContentEditable ? oa.textOf(el) : (el.value || ''))),
        checked: !!el.checked,
        options, accept: el.getAttribute('accept') || '', multiple: !!el.multiple,
        max_length: Number(el.getAttribute('maxlength') || 0) || 0,
        custom_select: custom, searchable: oa.isSearchable(el, custom), hidden: !oa.visible(el),
        in_dialog: !!oa.dialogOf(el), dialog_login: oa.dialogLogin(el),
        order: order++,
        in_iframe: el.ownerDocument !== document,
        in_shadow: !!(el.getRootNode && el.getRootNode() !== el.ownerDocument),
      });
      if (type === 'checkbox' && /同意|隐私|承诺|真实|条款|授权/.test(labelText)) {
        consent.push({id, label: labelText, checked: !!el.checked});
      }
    }
  }
  const seenButtons = new Set();
  for (const {root} of roots) {
    let nodes = [];
    try { nodes = root.querySelectorAll('button, input[type=submit], input[type=button], [role=button]'); } catch (e) { nodes = []; }
    for (const el of nodes) {
      if (buttons.length >= 120) break;
      if (seenButtons.has(el)) continue;
      seenButtons.add(el);
      if (!oa.visible(el)) continue;
      const text = (oa.textOf(el) || el.value || el.getAttribute('aria-label') || el.getAttribute('title') || '').trim();
      if (!text) continue;
      const id = assign(el);
      const dialog = el.closest('[role="dialog"], .ant-modal, .el-dialog, .modal, .layui-layer, .weui-dialog, .confirm-dialog');
      buttons.push({
        id, text: text.slice(0, 100), disabled: !!el.disabled,
        tag: el.tagName.toLowerCase(),
        type: (el.getAttribute('type') || '').toLowerCase(),
        in_dialog: !!dialog,
      });
    }
    let anchors = [];
    try { anchors = root.querySelectorAll('a[href]'); } catch (e) { anchors = []; }
    for (const el of anchors) {
      if (links.length >= 80) break;
      if (!oa.visible(el)) continue;
      const text = oa.textOf(el);
      if (!text || text.length > 50) continue;
      const href = el.href || '';
      if (!href || href.indexOf('javascript:') === 0) continue;
      if (!/申请|投递|提交|登录|注册|下一步|保存|继续|上传|校园|招聘|职位|岗位|实习|job|apply|career|recruit/i.test(text + ' ' + href)) continue;
      const id = assign(el);
      links.push({id, text: text.slice(0, 100), href});
    }
  }
  const bodyText = String(document.body ? document.body.innerText : '').replace(/\n{3,}/g, '\n\n').trim();
  const loginEvidence = [];
  if (document.querySelector('input[type=password]')) loginEvidence.push('密码输入框');
  if (/扫码登录|账号登录|验证码登录|请先登录|登录后投递|登录后查看|请登录后/.test(bodyText)) loginEvidence.push('登录提示文案');
  if (/(^|\.)login\.|signin|passport|\/sso|auth/i.test(location.href) && /登录/.test(bodyText)) loginEvidence.push('登录页 URL');
  const loggedInHint = /退出登录|我的简历|个人中心/.test(bodyText);
  if (loggedInHint) loginEvidence.push('已登录提示');
  const captchaEvidence = [];
  try {
    const captchaNodes = document.querySelectorAll('img[src*="captcha" i], iframe[src*="captcha" i], [class*="captcha" i], [id*="captcha" i]');
    for (const node of captchaNodes) {
      if (oa.visible(node)) { captchaEvidence.push('验证码组件'); break; }
    }
  } catch (e) {}
  if (/人机验证|安全验证|拖动滑块|滑动验证|请完成验证|图形验证码|图片验证码/.test(bodyText)) captchaEvidence.push('验证提示文案');
  const appliedEvidence = [];
  for (const button of buttons) {
    if (/已投递|已申请|已提交|投递成功|申请成功/.test(button.text)) appliedEvidence.push(button.text);
  }
  if (/投递成功|申请成功|已成功投递/.test(bodyText)) appliedEvidence.push('页面提示已投递');
  const termsEvidence = consent.map((item) => item.label);
  try {
    const termLinks = document.querySelectorAll('a');
    for (const node of termLinks) {
      const text = oa.textOf(node);
      if (/隐私政策|用户协议|个人信息保护|真实性承诺/.test(text) && oa.visible(node)) termsEvidence.push(text.slice(0, 100));
    }
  } catch (e) {}
  const validationErrors = [];
  try {
    const errorNodes = document.querySelectorAll(
      '.ant-form-item-explain-error, .el-form-item__error, .invalid-feedback, .form-error, ' +
      '[class*="error-message"], [class*="errorText"], [class*="field-error"], [role="alert"]'
    );
    for (const node of errorNodes) {
      if (!oa.visible(node)) continue;
      const text = oa.textOf(node);
      if (text && text.length <= 200) validationErrors.push(text);
      if (validationErrors.length >= 20) break;
    }
  } catch (e) {}
  const successEvidence = [];
  if (/提交成功|投递成功|申请成功|投递已完成|简历已投递|报名成功/.test(bodyText)) successEvidence.push('页面成功提示');
  if (/申请已提交|简历投递成功/.test(document.title || '')) successEvidence.push('页面标题成功提示');
  const iframes = [];
  const scanFrames = (doc, depth) => {
    if (depth > 3) return;
    let frames = [];
    try { frames = doc.querySelectorAll('iframe'); } catch (e) { return; }
    for (const frame of frames) {
      let sameOrigin = false;
      let childDoc = null;
      try { childDoc = frame.contentDocument; sameOrigin = !!childDoc; } catch (e) { sameOrigin = false; }
      const rect = frame.getBoundingClientRect();
      iframes.push({
        src: frame.src || '',
        same_origin: sameOrigin,
        visible: rect.width > 1 && rect.height > 1,
        width: Math.round(rect.width),
        height: Math.round(rect.height),
      });
      if (childDoc) scanFrames(childDoc, depth + 1);
    }
  };
  scanFrames(document, 0);
  const forms = Array.from(document.forms || []).map((form) => ({
    id: form.id || '', action: form.action || '', method: (form.method || '').toUpperCase(),
  }));
  const sections = [];
  const seenSections = new Set();
  for (const list of sectionsByRoot) {
    for (const item of list) {
      if (seenSections.has(item.text)) continue;
      seenSections.add(item.text);
      sections.push(item.text);
      if (sections.length >= 80) break;
    }
  }
  const dialogs = [];
  const seenDialogs = new Set();
  let loginModalVisible = false;
  const dialogSelector = [
    '[role="dialog"]', '.ant-modal', '.el-dialog', '.modal', '.layui-layer', '.weui-dialog', '.confirm-dialog',
    '[class*="modal" i]', '[class*="dialog" i]',
  ].join(',');
  for (const {root} of roots) {
    let nodes = [];
    try { nodes = root.querySelectorAll(dialogSelector); } catch (e) { nodes = []; }
    for (const node of nodes) {
      if (!oa.visible(node)) continue;
      const text = oa.textOf(node);
      if (!text || text.length > 3000) continue;
      const key = text.slice(0, 120);
      if (seenDialogs.has(key)) continue;
      seenDialogs.add(key);
      const isLogin = /登录|注册|验证码|密码|sign in|log in|login/i.test(text);
      dialogs.push({text: text.slice(0, 400), login: isLogin});
      if (isLogin) loginModalVisible = true;
      if (dialogs.length >= 10) break;
    }
    if (dialogs.length >= 10) break;
  }
  const tech = {
    scripts: Array.from(document.querySelectorAll('script[src]')).map((node) => node.src).filter(Boolean).slice(0, 60),
    class_hint: [
      (document.body && document.body.className) || '',
      (document.documentElement && document.documentElement.className) || '',
      Array.from(document.querySelectorAll('[class*="mokahr" i], [class*="beisen" i], [class*="dayee" i], [class*="gllue" i]'))
        .slice(0, 5).map((node) => node.className).join(' '),
    ].join(' ').slice(0, 800),
  };
  const headings = Array.from(document.querySelectorAll('h1, h2, h3')).slice(0, 15)
    .map((node) => oa.textOf(node)).filter(Boolean).map((text) => text.slice(0, 120));
  let jobPosting = null;
  try {
    const scripts = document.querySelectorAll('script[type="application/ld+json"]');
    for (const script of scripts) {
      const data = JSON.parse(script.textContent || 'null');
      const items = Array.isArray(data) ? data : (data && data['@graph'] ? data['@graph'] : [data]);
      for (const item of items) {
        if (!item || String(item['@type'] || '').indexOf('JobPosting') < 0) continue;
        const location = item.jobLocation || item.jobLocationType || '';
        jobPosting = {
          title: item.title || '',
          company: (item.hiringOrganization || {}).name || '',
          location: typeof location === 'string' ? location : (location.address || location.name || ''),
          date_posted: item.datePosted || '',
        };
        break;
      }
      if (jobPosting) break;
    }
  } catch (e) {}
  return JSON.stringify({
    url: location.href,
    title: document.title || '',
    page_text: bodyText.slice(0, __MAX_TEXT__),
    fields, buttons, links,
    consent_checkboxes: consent,
    forms,
    iframes,
    sections,
    dialogs,
    tech,
    headings,
    job_posting: jobPosting,
    signals: {
      login_evidence: loginEvidence,
      logged_in_hint: loggedInHint,
      login_modal_visible: loginModalVisible,
      captcha_evidence: captchaEvidence,
      already_applied_evidence: appliedEvidence,
      terms_evidence: termsEvidence,
      validation_errors: validationErrors,
      success_evidence: successEvidence,
    },
  });
})()
"""

FILL_JS = r"""
(() => {
  const oa = window.__oa;
  if (!oa) return JSON.stringify({error: 'helper_missing'});
  const actions = __ACTIONS__;
  const results = [];
  const setNative = (el, value) => {
    el.focus();
    if (el.isContentEditable) {
      el.textContent = value;
      el.dispatchEvent(new InputEvent('input', {bubbles: true, data: value, inputType: 'insertText'}));
      el.dispatchEvent(new Event('change', {bubbles: true}));
      el.dispatchEvent(new Event('blur', {bubbles: true}));
      return;
    }
    const win = el.ownerDocument.defaultView;
    const proto = el.tagName === 'TEXTAREA' ? win.HTMLTextAreaElement.prototype : win.HTMLInputElement.prototype;
    const desc = Object.getOwnPropertyDescriptor(proto, 'value');
    if (desc && desc.set) desc.set.call(el, value);
    else el.value = value;
    el.dispatchEvent(new Event('input', {bubbles: true}));
    el.dispatchEvent(new Event('change', {bubbles: true}));
    el.dispatchEvent(new Event('blur', {bubbles: true}));
  };
  const setSelect = (el, desired) => {
    const wanted = oa.norm(desired);
    const options = Array.from(el.options || []);
    let option = options.find((item) => String(item.value) === String(desired));
    if (!option) option = options.find((item) => oa.norm(item.text) === wanted);
    if (!option) option = options.find((item) => oa.norm(item.text).indexOf(wanted) >= 0);
    if (!option) option = options.find((item) => oa.norm(item.value).indexOf(wanted) >= 0);
    if (!option) return {ok: false, error: 'option_not_found', options: options.slice(0, 20).map((item) => oa.textOf(item))};
    const win = el.ownerDocument.defaultView;
    const proto = win.HTMLSelectElement.prototype;
    const desc = Object.getOwnPropertyDescriptor(proto, 'value');
    if (desc && desc.set) desc.set.call(el, option.value);
    else el.value = option.value;
    el.dispatchEvent(new Event('input', {bubbles: true}));
    el.dispatchEvent(new Event('change', {bubbles: true}));
    el.dispatchEvent(new Event('blur', {bubbles: true}));
    return {ok: true, method: 'select', selected: oa.textOf(option) || option.value};
  };
  const boolValue = (value) => {
    const text = oa.norm(value);
    if (['是', '有', '接受', '同意', '愿意', '服从', '可接受', 'true', '1', 'yes', 'y'].indexOf(text) >= 0) return true;
    if (['否', '无', '不接受', '不同意', '不愿意', '不服从', 'false', '0', 'no', 'n'].indexOf(text) >= 0) return false;
    return null;
  };
  const pickChoice = (group, desired) => {
    const wanted = oa.norm(desired);
    const bool = boolValue(desired);
    let loose = null;
    for (const item of group) {
      const label = oa.norm(oa.label(item));
      const value = oa.norm(item.value);
      const combined = label + ' ' + value;
      if (value === wanted || label === wanted) return item;
      if (bool !== null) {
        const itemBool = boolValue(item.value) !== null ? boolValue(item.value) : boolValue(oa.label(item));
        if (itemBool === bool) return item;
      }
      if (!loose && (combined.indexOf(wanted) >= 0 || wanted.indexOf(value) >= 0 || wanted.indexOf(label) >= 0)) loose = item;
    }
    return loose;
  };
  for (const action of actions) {
    const el = oa.find(action.id);
    if (!el) {
      results.push({id: action.id, ok: false, error: 'not_found'});
      continue;
    }
    if (el.disabled) {
      results.push({id: action.id, ok: false, error: 'disabled'});
      continue;
    }
    const values = [];
    if (action.value != null) values.push(String(action.value));
    if (Array.isArray(action.alternatives)) {
      for (const item of action.alternatives) if (item != null) values.push(String(item));
    }
    const value = values.length ? values[0] : '';
    try {
      if (action.kind === 'file') {
        results.push({id: action.id, ok: false, error: 'use_upload'});
        continue;
      }
      if (action.custom_select) {
        if (action.kind === 'select' || action.kind === 'input') {
          results.push({id: action.id, ok: false, error: 'custom_select_requires_click'});
          continue;
        }
      }
      if (action.kind === 'radio') {
        const name = el.getAttribute('name') || '';
        const root = el.getRootNode ? el.getRootNode() : el.ownerDocument;
        let group = [el];
        if (name) {
          try {
            group = Array.from(root.querySelectorAll('input[type=radio]')).filter((item) => (item.getAttribute('name') || '') === name);
          } catch (e) { group = [el]; }
        }
        if (!group.length) group = [el];
        let chosen = null;
        for (const desired of values) {
          chosen = pickChoice(group, desired);
          if (chosen) break;
        }
        if (!chosen) {
          results.push({id: action.id, ok: false, error: 'option_not_found', options: group.map((item) => oa.label(item))});
          continue;
        }
        const clickTarget = (chosen.labels && chosen.labels[0]) ? chosen.labels[0] : chosen;
        clickTarget.click();
        if (!chosen.checked) {
          chosen.checked = true;
          chosen.dispatchEvent(new Event('change', {bubbles: true}));
        }
        results.push({id: action.id, ok: true, method: 'radio', selected: oa.label(chosen) || chosen.value});
        continue;
      }
      if (action.kind === 'checkbox') {
        let shouldCheck = null;
        for (const desired of values) {
          const bool = boolValue(desired);
          if (bool !== null) { shouldCheck = bool; break; }
          const label = oa.norm(oa.label(el));
          const wantedLabel = oa.norm(desired);
          if (wantedLabel && (label.indexOf(wantedLabel) >= 0 || wantedLabel.indexOf(label) >= 0)) {
            shouldCheck = true;
            break;
          }
        }
        if (shouldCheck === null) {
          results.push({id: action.id, ok: false, error: 'checkbox_value_unclear', options: [oa.label(el)]});
          continue;
        }
        if (!!el.checked !== shouldCheck) {
          const clickTarget = (el.labels && el.labels[0]) ? el.labels[0] : el;
          clickTarget.click();
          if (!!el.checked !== shouldCheck) {
            el.checked = shouldCheck;
            el.dispatchEvent(new Event('change', {bubbles: true}));
          }
        }
        results.push({id: action.id, ok: true, method: 'checkbox', selected: shouldCheck});
        continue;
      }
      if (action.kind === 'select') {
        let outcome = null;
        for (const desired of values) {
          outcome = setSelect(el, desired);
          if (outcome.ok) break;
        }
        results.push(Object.assign({id: action.id}, outcome || {ok: false, error: 'option_not_found'}));
        continue;
      }
      setNative(el, value);
      results.push({id: action.id, ok: true, method: 'text'});
    } catch (e) {
      results.push({id: action.id, ok: false, error: String(e && e.message ? e.message : e)});
    }
  }
  return JSON.stringify({results});
})()
"""

SELECT_OPTION_JS = r"""
(() => {
  const oa = window.__oa;
  if (!oa) return JSON.stringify({error: 'helper_missing'});
  const el = oa.find(__OA_ID__);
  if (!el) return JSON.stringify({error: 'not_found'});
  el.scrollIntoView({block: 'center'});
  const rect = el.getBoundingClientRect();
  return JSON.stringify({ok: true, x: rect.left + rect.width / 2, y: rect.top + rect.height / 2});
})()
"""

TYPE_INTO_FIELD_JS = r"""
(() => {
  const oa = window.__oa;
  if (!oa) return JSON.stringify({error: 'helper_missing'});
  const el = oa.find(__OA_ID__);
  if (!el) return JSON.stringify({error: 'not_found'});
  el.focus();
  const value = __VALUE__;
  const win = el.ownerDocument.defaultView;
  const proto = win.HTMLInputElement.prototype;
  const desc = Object.getOwnPropertyDescriptor(proto, 'value');
  if (desc && desc.set) desc.set.call(el, value);
  else el.value = value;
  el.dispatchEvent(new Event('input', {bubbles: true}));
  el.dispatchEvent(new Event('change', {bubbles: true}));
  return JSON.stringify({ok: true, value: el.value || ''});
})()
"""

OPTIONS_JS = r"""
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
"""

CLICK_PREPARE_JS = r"""
(() => {
  const oa = window.__oa;
  if (!oa) return JSON.stringify({error: 'helper_missing'});
  const el = oa.find(__OA_ID__);
  if (!el) return JSON.stringify({ok: false, error: 'not_found'});
  el.scrollIntoView({block: 'center', inline: 'center'});
  const rect = el.getBoundingClientRect();
  return JSON.stringify({ok: true, x: rect.left + rect.width / 2, y: rect.top + rect.height / 2, visible: oa.visible(el)});
})()
"""

CLICK_TEXT_JS = r"""
(() => {
  const oa = window.__oa;
  if (!oa) return JSON.stringify({error: 'helper_missing'});
  const wanted = oa.norm(__TEXT__);
  const candidates = [];
  for (const {root} of oa.allRoots()) {
    let nodes = [];
    try { nodes = root.querySelectorAll('button, input[type=submit], input[type=button], [role=button], a'); } catch (e) { nodes = []; }
    for (const el of nodes) {
      if (!oa.visible(el)) continue;
      const text = oa.textOf(el) || el.value || el.getAttribute('aria-label') || '';
      if (!text) continue;
      const normalized = oa.norm(text);
      if (normalized === wanted) {
        el.scrollIntoView({block: 'center'});
        const rect = el.getBoundingClientRect();
        return JSON.stringify({ok: true, text: text.slice(0, 100), x: rect.left + rect.width / 2, y: rect.top + rect.height / 2});
      }
      if (normalized.indexOf(wanted) >= 0) candidates.push({el, text, normalized});
    }
  }
  if (candidates.length) {
    const chosen = candidates[0];
    chosen.el.scrollIntoView({block: 'center'});
    const rect = chosen.el.getBoundingClientRect();
    return JSON.stringify({ok: true, text: chosen.text.slice(0, 100), x: rect.left + rect.width / 2, y: rect.top + rect.height / 2});
  }
  return JSON.stringify({ok: false, error: 'not_found'});
})()
"""

CURRENT_TEXT_JS = r"""
(() => {
  const oa = window.__oa;
  if (!oa) return JSON.stringify({error: 'helper_missing'});
  const el = oa.find(__OA_ID__);
  if (!el) return JSON.stringify({error: 'not_found'});
  let display = '';
  const item = oa.itemOf(el);
  if (item) {
    const selectors = [
      '[class*="Select-value" i]', '[class*="selectValue" i]', '[class*="selection-item" i]',
      '[class*="selectionItem" i]', '.ant-select-selection-item', '.el-select__selected-item',
      '.el-select__placeholder', '[class*="selected-value" i]', '[class*="selectedValue" i]',
      '[class*="current-value" i]', '[class*="currentValue"]',
    ].join(',');
    let node = null;
    try { node = item.querySelector(selectors); } catch (e) { node = null; }
    if (node) {
      const text = oa.textOf(node);
      if (text && !/请选择|请输入/.test(text)) display = text;
    }
  }
  return JSON.stringify({
    text: oa.textOf(el),
    value: el.value || '',
    display,
    item_text: item ? oa.textOf(item).slice(0, 200) : '',
  });
})()
"""

DISPATCH_CHANGE_JS = r"""
(() => {
  const oa = window.__oa;
  if (!oa) return false;
  const el = oa.find(__OA_ID__);
  if (!el) return false;
  el.dispatchEvent(new Event('input', {bubbles: true}));
  el.dispatchEvent(new Event('change', {bubbles: true}));
  return true;
})()
"""

UPLOAD_OBJECT_JS = r"""
(() => {
  const oa = window.__oa;
  if (!oa) return null;
  return oa.find(__OA_ID__);
})()
"""


def ensure_helper(tab: CDPTab) -> None:
    for _ in range(3):
        try:
            if tab.evaluate(HELPER_JS, timeout=15):
                return
        except CDPError:
            pass
        time.sleep(0.2)
    raise ToolError("page_not_ready", "页面脚本尚未就绪，请稍后重试或重新打开页面")


def inspect_page(tab: CDPTab, *, max_text: int = 6000) -> dict[str, Any]:
    ensure_helper(tab)
    expression = INSPECT_JS.replace("__MAX_TEXT__", str(max(500, int(max_text))))
    try:
        raw = tab.evaluate(expression, timeout=30)
    except CDPError as exc:
        raise ToolError("inspect_failed", f"读取页面结构失败: {exc}") from exc
    data = _parse_js_payload(raw, "inspect_failed")
    if data.get("error"):
        raise ToolError("inspect_failed", f"读取页面结构失败: {data['error']}")
    data["page_text"] = redact_text(str(data.get("page_text") or ""))
    for field in data.get("fields") or []:
        haystack = " ".join(
            str(field.get(key) or "") for key in ("label", "name", "placeholder", "aria_label", "context")
        )
        if _is_sensitive_label(haystack):
            field["value"] = mask_sensitive_value(str(field.get("value") or ""))
    data["field_count"] = len(data.get("fields") or [])
    data["button_count"] = len(data.get("buttons") or [])
    return data


def fill_fields(tab: CDPTab, actions: list[dict[str, Any]]) -> list[dict[str, Any]]:
    if not actions:
        return []
    ensure_helper(tab)
    payload = json.dumps(actions, ensure_ascii=False)
    expression = FILL_JS.replace("__ACTIONS__", payload)
    try:
        raw = tab.evaluate(expression, timeout=45)
    except CDPError as exc:
        raise ToolError("fill_failed", f"填写页面失败: {exc}") from exc
    data = _parse_js_payload(raw, "fill_failed")
    if data.get("error"):
        raise ToolError("fill_failed", f"填写页面失败: {data['error']}")
    results = data.get("results") or []
    by_id = {str(item.get("id")): item for item in results if isinstance(item, dict)}
    merged: list[dict[str, Any]] = []
    for action in actions:
        result = dict(by_id.get(str(action.get("id")), {"ok": False, "error": "missing_result"}))
        result.update({
            "semantic": action.get("semantic") or "",
            "label": action.get("label") or "",
            "kind": action.get("kind") or "",
        })
        if result.get("ok"):
            result["value_preview"] = _preview_value(str(action.get("value") or ""), action.get("semantic") or "")
        merged.append(result)
    return merged


def select_custom_option(
    tab: CDPTab,
    field: dict[str, Any],
    values: str | list[str],
) -> dict[str, Any]:
    ensure_helper(tab)
    oa_id = str(field.get("id") or "")
    candidates = [values] if isinstance(values, str) else [str(item) for item in values if str(item)]
    if not candidates:
        return {"ok": False, "error": "empty_value"}
    clicked = _parse_js_payload(
        tab.evaluate(SELECT_OPTION_JS.replace("__OA_ID__", json.dumps(oa_id)), timeout=15),
        "select_failed",
    )
    if clicked.get("error"):
        return {"ok": False, "error": clicked["error"]}
    try:
        tab.click_at(float(clicked.get("x") or 0), float(clicked.get("y") or 0))
    except (TypeError, ValueError):
        pass
    last_options: list[str] = []
    reclicked = False
    searchable = bool(field.get("searchable"))
    typed: set[str] = set()
    deadline = time.time() + (5.5 if searchable else 3.5)
    while time.time() < deadline:
        time.sleep(0.4)
        opened = False
        for value in candidates:
            if searchable and value not in typed:
                typed.add(value)
                _type_into_field(tab, oa_id, value)
                time.sleep(0.45)
            options = _parse_js_payload(
                tab.evaluate(OPTIONS_JS.replace("__VALUE__", json.dumps(value, ensure_ascii=False)), timeout=15),
                "select_failed",
            )
            last_options = [str(item) for item in options.get("options") or []]
            if options.get("error"):
                if last_options:
                    opened = True
                continue
            opened = True
            x, y = options.get("x"), options.get("y")
            if not isinstance(x, (int, float)) or not isinstance(y, (int, float)):
                return {"ok": False, "error": "option_without_position"}
            tab.click_at(float(x), float(y))
            time.sleep(0.3)
            current = _parse_js_payload(
                tab.evaluate(CURRENT_TEXT_JS.replace("__OA_ID__", json.dumps(oa_id)), timeout=15),
                "select_failed",
            )
            current_text = str(current.get("display") or current.get("text") or current.get("value") or "")
            return {
                "ok": True,
                "selected": options.get("text") or "",
                "current": current_text,
                "verified": _option_matches(current_text, candidates),
                "searchable": searchable,
            }
        if not opened and not reclicked:
            reclicked = True
            try:
                tab.click_at(float(clicked.get("x") or 0), float(clicked.get("y") or 0))
            except (TypeError, ValueError):
                pass
    return {"ok": False, "error": "option_not_found", "options": last_options[:20]}


def _type_into_field(tab: CDPTab, oa_id: str, value: str) -> bool:
    expression = (
        TYPE_INTO_FIELD_JS
        .replace("__OA_ID__", json.dumps(oa_id))
        .replace("__VALUE__", json.dumps(value, ensure_ascii=False))
    )
    try:
        payload = _parse_js_payload(tab.evaluate(expression, timeout=15), "select_failed")
    except (CDPError, ToolError):
        return False
    return bool(payload.get("ok"))


def _option_matches(current: str, candidates: list[str]) -> bool:
    normalized = normalize_label(current)
    return any(
        normalize_label(candidate) and normalize_label(candidate) in normalized
        for candidate in candidates
    )


def click_oa(tab: CDPTab, oa_id: str) -> dict[str, Any]:
    ensure_helper(tab)
    prepared = _parse_js_payload(
        tab.evaluate(CLICK_PREPARE_JS.replace("__OA_ID__", json.dumps(oa_id)), timeout=15),
        "click_failed",
    )
    if not prepared.get("ok"):
        return {"ok": False, "error": prepared.get("error") or "not_found"}
    try:
        tab.click_at(float(prepared["x"]), float(prepared["y"]))
    except (KeyError, TypeError, ValueError):
        pass
    time.sleep(0.2)
    return {"ok": True, "text": prepared.get("text") or ""}


def click_text(tab: CDPTab, text: str) -> dict[str, Any]:
    ensure_helper(tab)
    payload = _parse_js_payload(
        tab.evaluate(CLICK_TEXT_JS.replace("__TEXT__", json.dumps(text, ensure_ascii=False)), timeout=15),
        "click_failed",
    )
    if not payload.get("ok"):
        return {"ok": False, "error": payload.get("error") or "not_found"}
    try:
        tab.click_at(float(payload["x"]), float(payload["y"]))
    except (KeyError, TypeError, ValueError):
        pass
    time.sleep(0.2)
    return {"ok": True, "text": payload.get("text") or text}


def upload_to_field(tab: CDPTab, oa_id: str, paths: list[str]) -> dict[str, Any]:
    existing = [str(Path(path).resolve()) for path in paths if Path(path).exists()]
    if not existing:
        return {"ok": False, "error": "file_not_found"}
    ensure_helper(tab)
    try:
        remote = tab.evaluate_raw(UPLOAD_OBJECT_JS.replace("__OA_ID__", json.dumps(oa_id)))
    except CDPError as exc:
        return {"ok": False, "error": str(exc)}
    object_id = remote.get("objectId")
    if not object_id:
        return {"ok": False, "error": "not_found"}
    if not tab.set_files_for_object(str(object_id), existing):
        return {"ok": False, "error": "set_files_failed"}
    try:
        tab.evaluate(DISPATCH_CHANGE_JS.replace("__OA_ID__", json.dumps(oa_id)))
    except CDPError:
        pass
    return {"ok": True, "files": [Path(path).name for path in existing]}


def upload_via_button(tab: CDPTab, oa_id: str, paths: list[str], *, timeout: float = 4) -> dict[str, Any]:
    existing = [str(Path(path).resolve()) for path in paths if Path(path).exists()]
    if not existing:
        return {"ok": False, "error": "file_not_found"}
    try:
        tab.command("Page.enable")
        tab.command("Page.setInterceptFileChooserDialog", {"enabled": True})
        clicked = click_oa(tab, oa_id)
        if not clicked.get("ok"):
            return clicked
        event = tab.wait_event("Page.fileChooserOpened", timeout=timeout)
        if not event or not event.get("backendNodeId"):
            return {"ok": False, "error": "file_chooser_not_opened"}
        tab.command(
            "DOM.setFileInputFiles",
            {"files": existing, "backendNodeId": event["backendNodeId"]},
        )
        return {"ok": True, "files": [Path(path).name for path in existing]}
    except CDPError as exc:
        return {"ok": False, "error": str(exc)}
    finally:
        try:
            tab.command("Page.setInterceptFileChooserDialog", {"enabled": False})
        except CDPError:
            pass


OTP_VERIFY_JS = r"""
(() => {
  const oa = window.__oa;
  if (!oa) return JSON.stringify({error: 'helper_missing'});
  const checks = __CHECKS__;
  const results = checks.map((item) => {
    const el = oa.find(item.id);
    if (!el) return {id: item.id, match: false, error: 'not_found'};
    const value = el.isContentEditable ? oa.textOf(el) : String(el.value || '');
    return {id: item.id, match: value === String(item.value)};
  });
  return JSON.stringify({results});
})()
"""

_OTP_TEXT_RE = re.compile(
    r"验证码|校验码|动态码|动态密码|确认码|验证代码|登录码|短信码|短信密码|"
    r"verification\s*code|one[-\s]?time(?:\s*code|\s*password)?|passcode|otp|"
    r"auth(?:entication)?\s*code|security\s*code",
    re.IGNORECASE,
)
_OTP_BAD_RE = re.compile(r"图形|图片|图像|captcha|滑块|滑动|拼图|算术|计算题|算式", re.IGNORECASE)
_OTP_NAME_RE = re.compile(
    r"^(?:otp|otpcode|otp_code|smscode|sms_code|verifycode|verify_code|"
    r"verificationcode|verification_code|authcode|auth_code|securitycode|security_code|vcode|code)$",
    re.IGNORECASE,
)
_OTP_INPUT_TYPES = {"", "text", "tel", "number", "numeric", "search"}


def find_otp_fields(page: dict[str, Any], *, field_id: str = "") -> list[dict[str, Any]]:
    fields = [item for item in page.get("fields") or [] if isinstance(item, dict)]
    if field_id:
        target = next((item for item in fields if str(item.get("id") or "") == field_id), None)
        if target is None or not _otp_text_input(target):
            return []
        return _otp_group(page, target)
    matched = [item for item in fields if _otp_candidate(item)]
    if len(matched) == 1 and _otp_max_length(matched[0]) <= 2:
        matched = _otp_group(page, matched[0])
    return sorted(matched, key=lambda item: int(item.get("order") or 0))


def otp_fill_actions(fields: list[dict[str, Any]], code: str) -> list[dict[str, Any]]:
    digits = re.sub(r"\D", "", str(code or ""))
    if not fields:
        raise ToolError("otp_field_not_found", "未找到验证码输入框")
    if not digits:
        raise ToolError("otp_invalid_code", "验证码内容为空")
    if len(fields) > 1 and all(1 <= _otp_max_length(item) <= 2 for item in fields):
        actions: list[dict[str, Any]] = []
        position = 0
        for field in fields:
            if position >= len(digits):
                break
            width = max(1, _otp_max_length(field))
            chunk = digits[position:position + width]
            position += len(chunk)
            actions.append(_otp_action(field, chunk))
        if position < len(digits) and actions:
            actions[-1]["value"] += digits[position:]
        return actions
    return [_otp_action(field, digits) for field in fields]


def verify_otp_fill(tab: CDPTab, actions: list[dict[str, Any]]) -> dict[str, Any]:
    if not actions:
        return {"ok": False, "results": []}
    ensure_helper(tab)
    checks = [{"id": action.get("id"), "value": action.get("value")} for action in actions]
    try:
        raw = tab.evaluate(OTP_VERIFY_JS.replace("__CHECKS__", json.dumps(checks, ensure_ascii=False)), timeout=15)
    except CDPError as exc:
        raise ToolError("otp_verify_failed", f"校验验证码填写结果失败: {exc}") from exc
    data = _parse_js_payload(raw, "otp_verify_failed")
    results = [item for item in data.get("results") or [] if isinstance(item, dict)]
    return {"ok": bool(results) and all(item.get("match") for item in results), "results": results}


def type_otp_fields(tab: CDPTab, actions: list[dict[str, Any]]) -> dict[str, Any]:
    results: list[dict[str, Any]] = []
    for action in actions:
        clicked = click_oa(tab, str(action.get("id") or ""))
        if not clicked.get("ok"):
            results.append({"id": action.get("id"), "ok": False, "error": clicked.get("error") or "focus_failed"})
            continue
        tab.press_key("SelectAll")
        tab.press_key("Backspace")
        typed = tab.type_text(str(action.get("value") or ""), human=True)
        time.sleep(0.08)
        results.append({"id": action.get("id"), "ok": typed})
    return {"ok": bool(results) and all(item.get("ok") for item in results), "results": results}


def _otp_candidate(field: dict[str, Any]) -> bool:
    if not _otp_text_input(field):
        return False
    text = " ".join(str(field.get(key) or "") for key in ("label", "placeholder", "aria_label", "group_label"))
    if _OTP_BAD_RE.search(text):
        return False
    if _OTP_TEXT_RE.search(text):
        return True
    name = str(field.get("name") or "")
    if not _OTP_NAME_RE.match(name):
        return False
    input_type = str(field.get("input_type") or "").lower()
    max_length = _otp_max_length(field)
    return input_type in {"tel", "number", "numeric"} or max_length == 0 or 4 <= max_length <= 8


def _otp_text_input(field: dict[str, Any]) -> bool:
    if field.get("hidden") or field.get("disabled") or field.get("readonly"):
        return False
    kind = str(field.get("kind") or "input")
    if kind not in {"input", "contenteditable"}:
        return False
    input_type = str(field.get("input_type") or "").lower()
    return input_type in _OTP_INPUT_TYPES


def _otp_group(page: dict[str, Any], seed: dict[str, Any]) -> list[dict[str, Any]]:
    if _otp_max_length(seed) > 2:
        return [seed]
    order = int(seed.get("order") or 0)
    section = str(seed.get("section") or "")
    group = [seed]
    for field in page.get("fields") or []:
        if field is seed or not isinstance(field, dict):
            continue
        if str(field.get("section") or "") != section:
            continue
        if abs(int(field.get("order") or 0) - order) > 8:
            continue
        if not _otp_text_input(field) or not 1 <= _otp_max_length(field) <= 2:
            continue
        text = " ".join(str(field.get(key) or "") for key in ("label", "placeholder", "aria_label", "group_label"))
        if _OTP_BAD_RE.search(text):
            continue
        group.append(field)
    return sorted(group, key=lambda item: int(item.get("order") or 0))


def _otp_max_length(field: dict[str, Any]) -> int:
    try:
        return int(field.get("max_length") or 0)
    except (TypeError, ValueError):
        return 0


def _otp_action(field: dict[str, Any], value: str) -> dict[str, Any]:
    return {
        "id": str(field.get("id") or ""),
        "kind": str(field.get("kind") or "input"),
        "value": value,
        "semantic": "otp",
        "label": str(field.get("label") or field.get("placeholder") or "")[:80],
    }


def _parse_js_payload(raw: Any, code: str) -> dict[str, Any]:
    if isinstance(raw, dict):
        return raw
    if isinstance(raw, str):
        try:
            data = json.loads(raw)
        except json.JSONDecodeError as exc:
            raise ToolError(code, f"页面返回了无法解析的数据: {exc}") from exc
        return data if isinstance(data, dict) else {"error": "invalid_payload"}
    if raw is None:
        raise ToolError("page_not_ready", "页面脚本无响应，可能正在跳转")
    raise ToolError(code, "页面返回了未知数据")


def _is_sensitive_label(text: str) -> bool:
    return bool(re.search(r"身份证|证件号|手机|电话|邮箱|地址|紧急联系|密码|验证码|银行卡", text))


def mask_sensitive_value(value: str) -> str:
    text = str(value or "")
    if not text:
        return ""
    if re.fullmatch(r"\d{17}[\dXx]", text):
        return mask_id(text)
    if re.fullmatch(r"1[3-9]\d{9}", text):
        return mask_phone(text)
    if "@" in text and re.fullmatch(r"[^@\s]+@[^@\s]+\.[^@\s]+", text):
        return mask_email(text)
    if len(text) <= 4:
        return "***"
    return f"{text[:2]}***{text[-2:]}"


def redact_text(text: str) -> str:
    value = str(text or "")
    value = re.sub(r"(?<!\d)(\d{17}[\dXx])(?!\d)", lambda m: mask_id(m.group(1)), value)
    value = re.sub(r"(?<!\d)(1[3-9]\d{9})(?!\d)", lambda m: mask_phone(m.group(1)), value)
    value = re.sub(
        r"(?<![\w.+-])([A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,})(?![\w.-])",
        lambda m: mask_email(m.group(1)),
        value,
    )
    return value


def _preview_value(value: str, semantic: str) -> str:
    if semantic == "otp":
        return "***"
    if semantic in {"id_number", "phone", "email", "address", "hukou", "emergency_contact_phone"}:
        return mask_sensitive_value(value)
    text = str(value or "").replace("\n", " ")
    return text[:80] + ("..." if len(text) > 80 else "")
