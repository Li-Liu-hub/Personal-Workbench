
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
