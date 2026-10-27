
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
