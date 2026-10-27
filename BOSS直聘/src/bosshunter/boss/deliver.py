"""BOSS delivery: click 立即沟通, send text and image messages, verify delivery."""

from __future__ import annotations

import json
import random
import time
from pathlib import Path
from typing import Any

from bosshunter.boss.search import (
    CHAT_URL,
    check_page_state,
    is_closed_page,
    job_id_from_url,
)
from bosshunter.browser import Browser, CDPError, CDPTab
from bosshunter.config import resolve_path
from bosshunter.errors import ToolError
from bosshunter.safety import SafetyError, SafetyState

CHAT_INPUT_SELECTOR = (
    '#chat-input, .chat-editor [contenteditable="true"], '
    '.chat-input [contenteditable="true"], .chat-editor textarea.input-area'
)
CHAT_SEND_SELECTOR = '.chat-editor .btn-send, .btn-send'

IMAGE_EXTENSIONS = {".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp"}

CHAT_BUTTON_SCRIPT = """
(() => {
    const selectors = [
        'a[redirect-url*="/web/geek/chat"]',
        'a[data-url*="/friend/add"]',
        'a.btn-startchat',
        '[ka="job_detail_chat"]',
        '[ka^="go_chat"]',
        '[ka*="gochat"]',
        '.op-btn-chat',
        '.btn-startchat-wrap'
    ];
    const candidates = selectors.flatMap((selector, priority) =>
        Array.from(document.querySelectorAll(selector)).map((el) => ({el, selector, priority}))
    );
    const elementState = (el) => {
        const rect = el.getBoundingClientRect();
        const style = getComputedStyle(el);
        const visible = !!(rect.width && rect.height && style.display !== 'none'
            && style.visibility !== 'hidden' && style.pointerEvents !== 'none');
        const inViewport = visible && rect.bottom > 0 && rect.right > 0
            && rect.top < innerHeight && rect.left < innerWidth;
        const x = Math.min(Math.max(rect.x + rect.width / 2, 0), innerWidth - 1);
        const y = Math.min(Math.max(rect.y + rect.height / 2, 0), innerHeight - 1);
        const top = inViewport ? document.elementFromPoint(x, y) : null;
        const topmost = !!(top && (top === el || el.contains(top)));
        return {visible, inViewport, topmost};
    };
    const score = (item) => {
        const el = item.el;
        const text = (el.innerText || el.textContent || '').trim();
        const ka = el.getAttribute('ka') || '';
        const redirectUrl = el.getAttribute('redirect-url') || '';
        const dataUrl = el.getAttribute('data-url') || '';
        const tagName = String(el.tagName || '').toLowerCase();
        let value = 0;
        const state = elementState(el);
        if (state.visible) value += 1000;
        if (state.inViewport) value += 500;
        if (state.topmost) value += 300;
        if (tagName === 'a') value += 200;
        if (redirectUrl.includes('/web/geek/chat')) value += 300;
        if (dataUrl.includes('/friend/add')) value += 250;
        if (el.classList && el.classList.contains('btn-startchat')) value += 120;
        if (text.includes('沟通')) value += 80;
        if (ka === 'job_detail_chat' || ka.includes('go_chat') || ka.includes('gochat')) value += 60;
        if (el.classList && el.classList.contains('btn-startchat-wrap')) value -= 100;
        return value - item.priority;
    };
    const matches = candidates.filter((item) => {
        const el = item.el;
        const text = (el.innerText || el.textContent || '').trim();
        const ka = el.getAttribute('ka') || '';
        return text.includes('沟通')
            || (el.getAttribute('redirect-url') || '').includes('/web/geek/chat')
            || (el.getAttribute('data-url') || '').includes('/friend/add')
            || ka === 'job_detail_chat'
            || ka.includes('go_chat')
            || ka.includes('gochat');
    }).sort((a, b) => score(b) - score(a));
    const btn = matches[0] && matches[0].el;
    if (!btn) {
        return JSON.stringify({
            success: false,
            error: 'no_chat_button',
            candidates: candidates.map((item) => ({
                text: (item.el.innerText || item.el.textContent || '').trim(),
                ka: item.el.getAttribute('ka'),
                className: String(item.el.className || ''),
                tagName: item.el.tagName,
                visible: elementState(item.el).visible
            }))
        });
    }
    btn.scrollIntoView({block: 'center', inline: 'center'});
    const rect = btn.getBoundingClientRect();
    btn.click();
    return JSON.stringify({
        success: true,
        interaction: 'dom_click',
        x: rect.x + rect.width / 2,
        y: rect.y + rect.height / 2,
        button_text: (btn.innerText || btn.textContent || '').trim(),
        klass: String(btn.className || ''),
        redirectUrl: btn.getAttribute('redirect-url'),
        dataUrl: btn.getAttribute('data-url')
    });
})()
"""

JS_JOB_HEADER = """
(() => {
    const text = (selector) => {
        const node = document.querySelector(selector);
        return node ? (node.innerText || node.textContent || '').trim() : '';
    };
    const lines = text('.sider-company').split(String.fromCharCode(10)).map((line) => line.trim()).filter(Boolean);
    const companyIndex = lines.indexOf('公司基本信息');
    return JSON.stringify({
        title: text('.info-primary .name h1') || text('.name h1') || (document.title || '').split('-')[0].trim(),
        company: text('.sider-company .company-info a') || text('.company-info .company-name')
            || (companyIndex >= 0 ? (lines[companyIndex + 1] || '') : '') || text('.company-name'),
        salary: text('.info-primary .salary') || text('.salary'),
        hr: (text('.job-boss-info .name') || text('.boss-info .name') || text('.job-boss-info h2'))
            .split(String.fromCharCode(10))[0].trim(),
        url: location.href
    });
})()
"""

JS_DETECT_POPUP = """
(() => {
    const visible = (element) => element && !!(
        element.offsetWidth || element.offsetHeight || element.getClientRects().length
    );
    const explicitPreset = Array.from(document.querySelectorAll('.greet-boss-pop, .greet-pop'))
        .find(visible);
    if (explicitPreset) {
        return JSON.stringify({success: true, popup: true, kind: 'preset_greeting'});
    }
    const startChat = Array.from(document.querySelectorAll('.dialog-wrap.startchat-dialog'))
        .find((element) => visible(element) && element.querySelector('textarea.input-area, textarea'));
    if (startChat) {
        return JSON.stringify({success: true, popup: true, kind: 'startchat_dialog'});
    }
    const preset = Array.from(document.querySelectorAll('.dialog-wrap'))
        .find((element) => {
            if (!visible(element)) return false;
            const text = (element.innerText || element.textContent || '').trim();
            const hasEditableGreeting = !!element.querySelector('textarea.input-area');
            return !hasEditableGreeting && /预设招呼语|默认招呼语|自动招呼语|打招呼语/.test(text);
        });
    if (preset) {
        return JSON.stringify({success: true, popup: true, kind: 'preset_greeting'});
    }
    return JSON.stringify({success: true, popup: false, kind: null});
})()
"""

JS_CONFIRM_PRESET = """
(() => {
    const visible = (element) => {
        if (!element) return false;
        const rect = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        return !!(rect.width && rect.height && style.display !== 'none'
            && style.visibility !== 'hidden' && style.pointerEvents !== 'none');
    };
    const dialogs = Array.from(document.querySelectorAll('.greet-boss-pop, .greet-pop, .dialog-wrap'))
        .filter(visible);
    const popup = dialogs.find((element) =>
        element.matches('.greet-boss-pop, .greet-pop')
        || /预设招呼语|默认招呼语|自动招呼语|打招呼语/.test(
            (element.innerText || element.textContent || '').trim()
        )
    );
    if (!popup) return JSON.stringify({success: false, error: 'preset_popup_missing'});
    const buttons = Array.from(popup.querySelectorAll(
        '[ka="dialog_confirm"], .btn-sure, button, [role="button"]'
    )).filter((element) => {
        if (!visible(element) || element.disabled || element.classList.contains('disabled')) return false;
        const text = (element.innerText || element.textContent || '').trim();
        return element.matches('[ka="dialog_confirm"], .btn-sure')
            || /确定|确认|继续|开始沟通|立即沟通/.test(text);
    });
    const button = buttons[0];
    if (!button) return JSON.stringify({success: false, error: 'preset_confirm_missing'});
    button.scrollIntoView({block: 'center', inline: 'center'});
    button.click();
    return JSON.stringify({success: true, action: 'preset_confirmed'});
})()
"""

JS_STARTCHAT_INPUT_RECT = """
(() => {
    const visible = (element) => {
        if (!element) return false;
        const rect = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        return !!(rect.width && rect.height && style.display !== 'none'
            && style.visibility !== 'hidden' && style.pointerEvents !== 'none');
    };
    const dialog = Array.from(document.querySelectorAll('.dialog-wrap.startchat-dialog')).find(visible);
    const input = dialog && Array.from(dialog.querySelectorAll('textarea.input-area, textarea')).find(visible);
    if (!input) return JSON.stringify({success: false, error: 'startchat_input_missing'});
    input.scrollIntoView({block: 'center', inline: 'center'});
    const rect = input.getBoundingClientRect();
    return JSON.stringify({
        success: true,
        x: rect.x + rect.width / 2,
        y: rect.y + rect.height / 2
    });
})()
"""

JS_STARTCHAT_VALUE = """
(() => {
    const visible = (element) => {
        if (!element) return false;
        const rect = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        return !!(rect.width && rect.height && style.display !== 'none'
            && style.visibility !== 'hidden' && style.pointerEvents !== 'none');
    };
    const dialog = Array.from(document.querySelectorAll('.dialog-wrap.startchat-dialog')).find(visible);
    const input = dialog && Array.from(dialog.querySelectorAll('textarea.input-area, textarea')).find(visible);
    if (!input) return JSON.stringify({success: false, error: 'startchat_input_missing'});
    return JSON.stringify({success: true, value: String(input.value || input.innerText || '')});
})()
"""

JS_STARTCHAT_SUBMIT_RECT = """
(() => {
    const visible = (element) => {
        if (!element) return false;
        const rect = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        return !!(rect.width && rect.height && style.display !== 'none'
            && style.visibility !== 'hidden' && style.pointerEvents !== 'none');
    };
    const dialog = Array.from(document.querySelectorAll('.dialog-wrap.startchat-dialog')).find(visible);
    if (!dialog) return JSON.stringify({success: false, error: 'startchat_dialog_missing'});
    const buttons = Array.from(dialog.querySelectorAll(
        '.send-message, [ka="dialog_confirm"], .btn-sure, .btn-send, .send-btn, button, [role="button"]'
    )).filter((element) => {
        if (!visible(element) || element.disabled || element.classList.contains('disabled')) return false;
        const text = (element.innerText || element.textContent || '').trim();
        return element.matches('.send-message, [ka="dialog_confirm"], .btn-sure, .btn-send, .send-btn')
            || /发送|确定|开始沟通|立即沟通/.test(text);
    });
    const button = buttons[0];
    if (!button) return JSON.stringify({success: false, error: 'startchat_submit_missing'});
    button.scrollIntoView({block: 'center', inline: 'center'});
    const rect = button.getBoundingClientRect();
    return JSON.stringify({success: true, x: rect.x + rect.width / 2, y: rect.y + rect.height / 2});
})()
"""

JS_CHAT_INPUT_VALUE = """
(() => {
    const input = document.querySelector('#chat-input')
        || document.querySelector('.chat-editor [contenteditable="true"]')
        || document.querySelector('.chat-input [contenteditable="true"]')
        || document.querySelector('.chat-editor textarea.input-area');
    if (!input) return JSON.stringify({success: false, error: 'no_chat_input'});
    const value = 'value' in input ? String(input.value || '') : String(input.innerText || input.textContent || '');
    const sendButton = document.querySelector('.chat-editor .btn-send, .btn-send');
    const disabled = !sendButton || sendButton.disabled || sendButton.classList.contains('disabled');
    return JSON.stringify({success: true, value, disabled, has_button: !!sendButton});
})()
"""

JS_CHAT_SUBMIT_FALLBACK = """
(() => {
    const input = document.querySelector('#chat-input')
        || document.querySelector('.chat-editor [contenteditable="true"]')
        || document.querySelector('.chat-input [contenteditable="true"]');
    if (!input) return JSON.stringify({success: false, error: 'no_chat_input'});
    let vue = null;
    let element = input;
    for (let index = 0; index < 15 && element; index += 1) {
        if (element.__vue__) { vue = element.__vue__; break; }
        element = element.parentElement;
    }
    if (!vue || typeof vue.handleSubmit !== 'function') {
        return JSON.stringify({success: false, error: 'legacy_submit_unavailable'});
    }
    input.innerText = __TEXT__;
    input.dispatchEvent(new InputEvent('input', {bubbles: true, inputType: 'insertText', data: __TEXT__}));
    if (vue._data) vue._data.enableSubmit = true;
    vue.handleSubmit();
    return JSON.stringify({success: true, action: 'chat_submitted'});
})()
"""

JS_MESSAGE_STATE = """
(() => {
    const normalize = (value) => String(value || '')
        .replace(/[\\u200b-\\u200f\\ufeff]/g, '')
        .replace(/\\s+/g, ' ')
        .trim();
    const removeDeliveryLabels = (value) => normalize(value)
        .replace(/(发送中|已读|未读|送达|发送成功|重试|重新发送)$/g, '')
        .trim();
    const textMatchesExpected = (value, expectedText) => {
        const text = removeDeliveryLabels(value);
        return text === expectedText || text.includes(expectedText);
    };
    const messageText = (node) => {
        const contentNode = node.querySelector(
            '.message-content, .text, .content, .message-text, '
            + '[class*="message-content"], [class*="msg-content"], [class*="text"]'
        );
        return normalize(contentNode
            ? contentNode.innerText || contentNode.textContent
            : node.innerText || node.textContent);
    };
    const expected = normalize(__TEXT__);
    const ownMessages = Array.from(document.querySelectorAll(
        '.chat-record .message-item.item-myself, .chat-record .item-myself, '
        + '.chat-record .message-item.item-self, .chat-record [class*="item-my"]'
    ));
    const matching = ownMessages.filter((node) => textMatchesExpected(messageText(node), expected));
    const messageList = document.querySelector('.chat-record');
    const vue = messageList && messageList.__vue__;
    const records = vue && Array.isArray(vue.list$) ? vue.list$ : [];
    const matchingRecords = records.filter((message) => {
        if (!message || !message.isSelf) return false;
        const text = message.text || message.lastText || message.content || message.message || message.body || '';
        return textMatchesExpected(text, expected);
    });
    if (!matching.length && !matchingRecords.length) {
        return JSON.stringify({success: true, state: 'missing'});
    }
    const domStates = matching.map((node) => {
        const statusNode = node.querySelector('.message-status');
        const statusClass = statusNode ? String(statusNode.className || '') : '';
        if (statusClass.includes('status-error')) return 'failed';
        if (statusClass.includes('status-loading')) return 'pending';
        return 'delivered';
    });
    const recordStates = matchingRecords.map((record) => {
        const status = Number(record.status);
        if (status === 4) return 'failed';
        if (status === 0) return 'pending';
        return 'delivered';
    });
    const states = [...domStates, ...recordStates];
    const state = states.includes('delivered') ? 'delivered'
        : states.includes('pending') ? 'pending' : 'failed';
    return JSON.stringify({success: true, state});
})()
"""

JS_OWN_MESSAGES = """
(() => {
    const own = Array.from(document.querySelectorAll(
        '.chat-record .message-item.item-myself, .chat-record .item-myself, '
        + '.chat-record .message-item.item-self, .chat-record [class*="item-my"]'
    ));
    const messageList = document.querySelector('.chat-record');
    const vue = messageList && messageList.__vue__;
    const records = vue && Array.isArray(vue.list$) ? vue.list$ : [];
    const recordCount = records.filter((message) => message && message.isSelf).length;
    return JSON.stringify({count: Math.max(own.length, recordCount)});
})()
"""

JS_OWN_IMAGES = """
(() => {
    const own = Array.from(document.querySelectorAll(
        '.chat-record .message-item.item-myself, .chat-record .item-myself, '
        + '.chat-record .message-item.item-self, .chat-record [class*="item-my"]'
    ));
    let domCount = 0;
    own.forEach((node) => {
        const images = Array.from(node.querySelectorAll('img, [style*="background-image"]'))
            .filter((element) => !element.closest(
                '.figure, .avatar, .head, [class*="avatar"], [class*="figure"]'
            ));
        if (images.length) domCount += 1;
    });
    const messageList = document.querySelector('.chat-record');
    const vue = messageList && messageList.__vue__;
    const records = vue && Array.isArray(vue.list$) ? vue.list$ : [];
    let recordCount = 0;
    records.forEach((message) => {
        if (!message || !message.isSelf) return;
        const body = message.body;
        const type = String(message.type || message.msgType || '');
        let hasImage = /image|img|picture|photo/i.test(type)
            || (typeof body === 'string' && /<img|image\\//i.test(body));
        if (!hasImage && body && typeof body === 'object') {
            hasImage = !!(body.image || body.img || body.picture);
        }
        if (!hasImage && typeof body === 'string' && body.trim().startsWith('{')) {
            try {
                const parsed = JSON.parse(body);
                hasImage = !!(parsed && (parsed.image || parsed.img || parsed.picture));
            } catch (err) {}
        }
        if (hasImage) recordCount += 1;
    });
    return JSON.stringify({count: Math.max(domCount, recordCount), dom: domCount, records: recordCount});
})()
"""

JS_CHAT_READY = """
(() => {
    const normalize = (value) => String(value || '').replace(/\\s+/g, '').toLowerCase();
    const company = normalize(__COMPANY__);
    const title = normalize(__TITLE__);
    const input = Boolean(document.querySelector(
        '#chat-input, .chat-editor [contenteditable="true"], '
        + '.chat-input [contenteditable="true"], .chat-editor textarea.input-area'
    ));
    const activeRoots = Array.from(new Set([
        document.querySelector('.chat-conversation'),
        document.querySelector('.friend-content.selected'),
        document.querySelector('.chat-container'),
        document.querySelector('.chat-header'),
        document.querySelector('.chat-top')
    ].filter(Boolean)));
    const activeText = normalize(activeRoots.map((element) =>
        element.innerText || element.textContent || ''
    ).join(' '));
    const companyMatch = !company || activeText.includes(company);
    const titleMatch = !title || activeText.includes(title);
    return JSON.stringify({
        chat: location.pathname.includes('/web/geek/chat'),
        input,
        matches: companyMatch || titleMatch,
        company_match: companyMatch,
        title_match: titleMatch,
        href: location.href
    });
})()
"""

JS_CHAT_LIST_ITEM = """
(() => {
    const hr = __HR__;
    const company = __COMPANY__;
    const items = Array.from(document.querySelectorAll('.friend-content'));
    const scored = items.map((el, index) => {
        const text = (el.innerText || '').replace(/\\s+/g, ' ');
        let score = 0;
        if (hr && text.includes(hr)) score += 100;
        if (company && text.includes(company)) score += 40;
        el.scrollIntoView({block: 'center'});
        const rect = el.getBoundingClientRect();
        return {
            text: text.slice(0, 160),
            score,
            index,
            x: rect.x + rect.width / 2,
            y: rect.y + rect.height / 2
        };
    }).filter((item) => item.score >= 40).sort((a, b) => b.score - a.score);
    if (!scored.length) return JSON.stringify({found: false, count: items.length});
    const best = scored[0];
    items[best.index].click();
    return JSON.stringify({found: true, best});
})()
"""

JS_PROBE_FILE_INPUT = """
(() => {
    const visible = (element) => {
        if (!element) return false;
        const rect = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        return !!(rect.width && rect.height && style.display !== 'none'
            && style.visibility !== 'hidden' && style.pointerEvents !== 'none');
    };
    const describe = (input) => {
        const accept = String(input.accept || '').toLowerCase();
        const looksImage = !accept || /image|\\.(jpg|jpeg|png|gif|webp|bmp)/.test(accept);
        let ancestors = '';
        let node = input.parentElement;
        for (let index = 0; index < 6 && node; index += 1) {
            ancestors += ' ' + String(node.className || '');
            node = node.parentElement;
        }
        const editor = document.querySelector('.chat-editor, .chat-container, .chat-dialog');
        let score = looksImage ? 100 : -1000;
        if (editor && editor.contains(input)) score += 200;
        if (visible(input)) score += 30;
        if (input.multiple) score += 10;
        if (/image|pic|photo|upload/i.test(ancestors)) score += 40;
        return {input, accept, multiple: !!input.multiple, visible: visible(input), score};
    };
    const inputs = Array.from(document.querySelectorAll('input[type=file]')).map(describe)
        .sort((a, b) => b.score - a.score);
    document.querySelectorAll('[data-bosshunter-upload]').forEach((element) => {
        element.removeAttribute('data-bosshunter-upload');
    });
    const best = inputs[0];
    if (best && best.score > 0) {
        best.input.setAttribute('data-bosshunter-upload', '1');
    }
    return JSON.stringify({
        found: !!(best && best.score > 0),
        multiple: !!(best && best.multiple),
        candidates: inputs.slice(0, 10).map((item) => ({
            accept: item.accept,
            multiple: item.multiple,
            visible: item.visible,
            score: item.score
        }))
    });
})()
"""

JS_PROBE_IMAGE_TRIGGER = """
(() => {
    const visible = (element) => {
        if (!element) return false;
        const rect = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        return !!(rect.width && rect.height && style.display !== 'none'
            && style.visibility !== 'hidden' && style.pointerEvents !== 'none');
    };
    const editor = document.querySelector('.chat-editor, .chat-container, .chat-input');
    const scope = editor || document;
    const nodes = Array.from(scope.querySelectorAll(
        'button, [role="button"], a, [class*="toolbar"] span, [class*="icon"], [class*="upload"]'
    ));
    const describe = (element) => {
        const text = (element.innerText || element.textContent || '').trim();
        const title = element.getAttribute('title') || '';
        const aria = element.getAttribute('aria-label') || '';
        const klass = String(element.className || '');
        const haystack = [text, title, aria, klass].join(' ');
        let score = 0;
        if (/图片|相册|照片/.test(haystack)) score += 200;
        if (/image|photo|picture|album|upload/i.test(haystack)) score += 120;
        if (/toolbar/i.test(klass)) score += 30;
        if (text.length > 8) score -= 40;
        return {element, text, title, aria, klass: klass.slice(0, 160), score};
    };
    const candidates = nodes.filter((element) => {
        if (!visible(element)) return false;
        const text = (element.innerText || element.textContent || '').trim();
        if (text.length > 12) return false;
        return true;
    }).map(describe).filter((item) => item.score > 0).sort((a, b) => b.score - a.score);
    document.querySelectorAll('[data-bosshunter-image-trigger]').forEach((element) => {
        element.removeAttribute('data-bosshunter-image-trigger');
    });
    const best = candidates[0];
    if (best) best.element.setAttribute('data-bosshunter-image-trigger', '1');
    return JSON.stringify({
        found: !!best,
        candidates: candidates.slice(0, 12).map((item) => ({
            text: item.text,
            title: item.title,
            aria: item.aria,
            klass: item.klass,
            score: item.score
        }))
    });
})()
"""

JS_PREVIEW_SEND_BUTTON = """
(() => {
    const visible = (element) => {
        if (!element) return false;
        const rect = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        return !!(rect.width && rect.height && style.display !== 'none'
            && style.visibility !== 'hidden' && style.pointerEvents !== 'none');
    };
    const containers = Array.from(document.querySelectorAll(
        '.dialog-wrap, [class*="dialog"], [class*="modal"], [class*="preview"], [class*="upload"]'
    )).filter((element) => visible(element) && element.querySelector('img'));
    const candidates = [];
    containers.forEach((container) => {
        if (container.closest('.chat-editor, .chat-record')) return;
        const buttons = Array.from(container.querySelectorAll(
            '.btn-send, .btn-sure, [ka="dialog_confirm"], button, [role="button"]'
        )).filter((element) => {
            if (!visible(element) || element.disabled || element.classList.contains('disabled')) return false;
            const text = (element.innerText || element.textContent || '').trim();
            return element.matches('.btn-send, .btn-sure, [ka="dialog_confirm"]')
                || /发送|确定|确认|继续/.test(text);
        });
        buttons.forEach((button) => {
            const text = (button.innerText || button.textContent || '').trim();
            let score = 0;
            if (button.matches('.btn-send, [ka="dialog_confirm"], .btn-sure')) score += 100;
            if (/发送/.test(text)) score += 50;
            candidates.push({button, text, score});
        });
    });
    candidates.sort((a, b) => b.score - a.score);
    document.querySelectorAll('[data-bosshunter-preview-send]').forEach((element) => {
        element.removeAttribute('data-bosshunter-preview-send');
    });
    const best = candidates[0];
    if (best) best.button.setAttribute('data-bosshunter-preview-send', '1');
    return JSON.stringify({
        found: !!best,
        candidates: candidates.slice(0, 8).map((item) => ({text: item.text, score: item.score}))
    });
})()
"""

JS_IMAGE_PREVIEW_VISIBLE = """
(() => {
    const visible = (element) => {
        if (!element) return false;
        const rect = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        return !!(rect.width && rect.height && style.display !== 'none'
            && style.visibility !== 'hidden');
    };
    const containers = Array.from(document.querySelectorAll(
        '.dialog-wrap, [class*="dialog"], [class*="modal"], [class*="preview"], [class*="upload"]'
    )).filter((element) => visible(element)
        && !element.closest('.chat-editor, .chat-record')
        && element.querySelector('img'));
    return JSON.stringify({visible: containers.length > 0});
})()
"""


def _parse(raw: Any) -> dict[str, Any]:
    if isinstance(raw, dict):
        return raw
    if isinstance(raw, str):
        try:
            value = json.loads(raw)
            return value if isinstance(value, dict) else {}
        except json.JSONDecodeError:
            return {}
    return {}


class BossDeliverer:
    def __init__(self, config: dict[str, Any], state: SafetyState, browser: Browser | None = None) -> None:
        self.config = config
        self.state = state
        self.browser = browser or Browser(config)
        self._random = random.SystemRandom()
        self.warnings: list[str] = []

    def deliver(
        self,
        job_url: str,
        *,
        greeting: str | None = None,
        images: list[str] | None = None,
        send_greeting: bool = True,
        send_images: bool = True,
        force: bool = False,
        dry_run: bool = False,
        wait: bool = False,
    ) -> dict[str, Any]:
        return self._execute(
            job_url,
            greeting=greeting,
            images=images,
            send_greeting=send_greeting,
            send_images=send_images,
            force=force,
            dry_run=dry_run,
            wait=wait,
        )

    def send_images(
        self,
        job_url: str,
        *,
        images: list[str] | None = None,
        message: str | None = None,
        force: bool = True,
        wait: bool = False,
    ) -> dict[str, Any]:
        if not images and not self.config["delivery"]["images"]:
            raise ToolError("no_images", "没有提供 images，且配置中的 delivery.images 为空")
        return self._execute(
            job_url,
            greeting=message,
            images=images,
            send_greeting=bool(message),
            send_images=True,
            force=force,
            dry_run=False,
            wait=wait,
        )

    # -- main flow ---------------------------------------------------------
    def _execute(
        self,
        job_url: str,
        *,
        greeting: str | None,
        images: list[str] | None,
        send_greeting: bool,
        send_images: bool,
        force: bool,
        dry_run: bool,
        wait: bool,
    ) -> dict[str, Any]:
        self.warnings = []
        url = str(job_url or "").strip()
        job_id = job_id_from_url(url)
        if not job_id:
            raise ToolError("invalid_job_url", "请提供完整的 BOSS 岗位详情链接（含 /job_detail/）")
        delivery_cfg = self.config["delivery"]
        greeting_text = (greeting if greeting is not None else delivery_cfg["greeting"]).strip()
        if len(greeting_text) > 500:
            raise ToolError("greeting_too_long", "消息超过 500 字，请缩短后再发送")

        image_values = images if images is not None else delivery_cfg["images"]
        image_paths = self._prepare_images(image_values) if send_images else []
        images_skipped_no_config = False
        if not dry_run:
            if send_images and not image_paths and images is None and not delivery_cfg["images"]:
                send_images = False
                images_skipped_no_config = True
            if send_greeting and not greeting_text:
                raise ToolError("no_greeting", "需要发送文字消息，但没有提供 greeting，且配置中的 delivery.greeting 为空")
            if send_images and not image_paths:
                raise ToolError("no_images", "需要发送图片，但没有提供 images，且配置中的 delivery.images 为空")
            if not send_greeting and not send_images:
                raise ToolError("nothing_to_send", "greeting 和 images 至少需要提供一项")
            if images_skipped_no_config:
                self.warnings.append("未配置 delivery.images，本次仅发送文字消息")

        previous = self.state.delivery_for(job_id)
        if not dry_run and not force and self.state.is_delivered(job_id):
            return {
                "skipped": True,
                "reason": "already_delivered",
                "job_id": job_id,
                "url": url,
                "delivery": previous,
                "message": "该岗位已有成功投递记录，未重复发送；如需强制重发请传 force=true",
            }
        if not dry_run:
            self._precheck_send(wait, force)

        try:
            self.state.reserve_page("chat")
        except SafetyError as exc:
            raise ToolError(exc.code, exc.message, **exc.details) from exc

        job_tab: CDPTab | None = None
        chat_tab: CDPTab | None = None
        adopted_new_chat = False
        header: dict[str, Any] = {"title": "", "company": "", "salary": ""}
        try:
            try:
                job_tab = self.browser.open(url, background=True)
            except CDPError as exc:
                raise ToolError("open_page_failed", f"无法打开岗位详情页: {exc}") from exc
            if not job_tab.wait_for_load(20):
                self._sleep(1.5, 2.5)
            check_page_state(self.config, self.state, job_tab)
            current_url = job_tab.current_url()
            if "/job_detail/" not in current_url or job_id_from_url(current_url) != job_id:
                self._sleep(2.0, 3.0)
                try:
                    job_tab.navigate(url, wait=True, timeout=20)
                    job_tab.wait_for_load(20)
                except CDPError:
                    pass
                self._sleep(1.0, 2.0)
                current_url = job_tab.current_url()
                if "/job_detail/" not in current_url or job_id_from_url(current_url) != job_id:
                    raise ToolError(
                        "job_page_redirected",
                        f"岗位详情页被安全校验或其他页面替代，已停止点击以免误投: {current_url}",
                    )
            if is_closed_page(job_tab):
                self._record_failure(job_id, url, header, "job_closed")
                raise ToolError("job_closed", "岗位已关闭、下架或停止招聘")
            header = self._read_job_header(job_tab)
            self._sleep(1.0, 2.0)

            if dry_run:
                button_state = _parse(job_tab.evaluate(CHAT_BUTTON_SCRIPT.replace("btn.click();", "")))
                return {
                    "dry_run": True,
                    "job_id": job_id,
                    "url": url,
                    "job": header,
                    "chat_button": button_state,
                    "would_send_greeting": bool(greeting_text),
                    "would_send_images": len(image_paths),
                    "message": "dry-run 只读取页面，不点击沟通、不发送任何内容",
                }

            existing_ids = {
                str(target.get("id"))
                for target in self.browser.chrome.list_targets()
                if target.get("id")
            }
            try:
                self.state.check_send_allowed(0.0)
            except SafetyError as exc:
                raise ToolError(exc.code, exc.message, **exc.details) from exc
            click_result = self._click_chat_button(job_tab)
            if not click_result.get("success"):
                if is_closed_page(job_tab):
                    self._record_failure(job_id, url, header, "job_closed")
                    raise ToolError("job_closed", "岗位已关闭、下架或停止招聘")
                raise ToolError(
                    "no_chat_button",
                    "页面上未找到可用的沟通按钮",
                    candidates=click_result.get("candidates", []),
                )
            contact_action = self._resolve_popup(job_tab, greeting_text)
            chat_tab, adopted_new_chat = self._wait_for_chat(
                job_tab,
                header,
                existing_ids,
                timeout=12 if contact_action == "no_popup" else 20,
            )
            if chat_tab is None and contact_action == "no_popup" and "/web/geek/chat" not in job_tab.current_url():
                # A DOM click can be ignored by the site's own handlers. Retry
                # once with a trusted mouse click, but only when no greeting
                # dialog was ever detected, so a duplicate cannot be sent.
                late_action = self._resolve_popup(job_tab, greeting_text)
                if late_action == "no_popup":
                    fresh = _parse(job_tab.evaluate(CHAT_BUTTON_SCRIPT.replace("btn.click();", "")))
                    if fresh.get("success"):
                        try:
                            clicked = job_tab.click_at(float(fresh["x"]), float(fresh["y"]))
                        except (TypeError, ValueError):
                            clicked = False
                        if clicked:
                            self._sleep(1.0, 2.0)
                            late_action = self._resolve_popup(job_tab, greeting_text)
                if late_action != "no_popup":
                    contact_action = late_action
                    self._sleep(1.2, 2.2)
                    chat_tab, adopted_new_chat = self._wait_for_chat(job_tab, header, existing_ids)
            if chat_tab is None and contact_action in {"first_contact_submitted", "preset_confirmed"}:
                chat_tab = self._open_chat_from_list(header)
            if chat_tab is None:
                if contact_action in {"first_contact_submitted", "preset_confirmed"}:
                    self.state.record_delivery(
                        job_id,
                        status="ambiguous",
                        title=str(header.get("title") or ""),
                        company=str(header.get("company") or ""),
                        url=url,
                        greeting=greeting_text if send_greeting else "",
                        images=[],
                        note="首次沟通已点击，但未能验证进入对应聊天会话",
                    )
                    raise ToolError(
                        "first_contact_unverified",
                        "首次沟通动作已执行，但未进入可验证的聊天会话；为避免重复已记录为待人工确认",
                        job_id=job_id,
                    )
                raise ToolError("chat_navigation_failed", "点击沟通后未进入聊天页面")
            if adopted_new_chat:
                self.browser.close(job_tab)
                job_tab = None
                try:
                    self.state.reserve_page("chat")
                except SafetyError as exc:
                    raise ToolError(exc.code, exc.message, **exc.details) from exc
            check_page_state(self.config, self.state, chat_tab)
            self._sleep(1.2, 2.2)

            greeting_state = "not_requested"
            greeting_message = ""
            if send_greeting and greeting_text:
                greeting_state, greeting_message = self._ensure_greeting(chat_tab, greeting_text, contact_action)
            if greeting_state == "unverified":
                self.state.record_delivery(
                    job_id,
                    status="ambiguous",
                    title=str(header.get("title") or ""),
                    company=str(header.get("company") or ""),
                    url=url,
                    greeting=greeting_text,
                    images=[],
                    note="文字消息提交后未能确认送达",
                )
                raise ToolError(
                    "greeting_unverified",
                    "文字消息已提交但无法确认送达；为避免重复已记录为待人工确认",
                    job_id=job_id,
                )
            greeting_evidence = greeting_state in {"sent", "already_present", "skipped_existing_conversation"}
            greeting_ok = not send_greeting or greeting_evidence
            if send_greeting and not greeting_ok:
                self._record_failure(job_id, url, header, greeting_message or "greeting_failed")
                raise ToolError(
                    "greeting_failed",
                    greeting_message or "文字消息发送失败",
                    job_id=job_id,
                    greeting_state=greeting_state,
                )

            images_sent = 0
            image_error: dict[str, Any] | None = None
            if send_images and image_paths:
                images_sent, image_error = self._send_images_in_chat(chat_tab, image_paths)

            image_ok = not send_images or images_sent == len(image_paths)
            initial_contact_sent = contact_action in {"preset_confirmed", "first_contact_submitted"}
            any_sent = greeting_evidence or initial_contact_sent or images_sent > 0
            if not any_sent:
                self._record_failure(
                    job_id,
                    url,
                    header,
                    str((image_error or {}).get("message") or "send_failed"),
                )
                raise ToolError(
                    str((image_error or {}).get("error") or "send_failed"),
                    str((image_error or {}).get("message") or "未能发送任何内容"),
                    job_id=job_id,
                    image_error=image_error,
                )

            partial = not greeting_ok or not image_ok
            note_parts = []
            if greeting_state == "skipped_existing_conversation":
                note_parts.append("已有会话，跳过重复招呼语")
            if contact_action == "preset_confirmed":
                note_parts.append("已确认平台预设招呼语")
            if contact_action == "first_contact_submitted":
                note_parts.append("已提交首次沟通招呼语")
            if image_error:
                note_parts.append(str(image_error.get("message") or image_error.get("error") or "图片发送失败"))
            record = self.state.record_delivery(
                job_id,
                status="sent",
                title=str(header.get("title") or ""),
                company=str(header.get("company") or ""),
                url=url,
                greeting=greeting_text if send_greeting else "",
                images=image_paths[:images_sent],
                note="；".join(note_parts),
            )
            if partial:
                self.warnings.append("消息只发送了一部分，请检查会话；图片可稍后用 boss_send_images 单独重试")
            return {
                "skipped": False,
                "partial": partial,
                "job_id": job_id,
                "url": url,
                "title": record.get("title", ""),
                "company": record.get("company", ""),
                "greeting": {
                    "requested": send_greeting,
                    "state": greeting_state,
                    "message": greeting_message,
                },
                "preset_greeting_sent": contact_action == "preset_confirmed",
                "images": {
                    "requested": len(image_paths) if send_images else 0,
                    "sent": images_sent,
                    "error": image_error,
                },
                "delivery": record,
                "warnings": self.warnings,
            }
        finally:
            if chat_tab is not None and chat_tab is not job_tab:
                self.browser.close(chat_tab)
            if job_tab is not None:
                self.browser.close(job_tab)

    # -- preparation -------------------------------------------------------
    def _prepare_images(self, values: Any) -> list[str]:
        if not isinstance(values, list):
            raise ToolError("invalid_images", "images 必须是图片路径数组")
        paths: list[str] = []
        for value in values:
            if not isinstance(value, str) or not value.strip():
                continue
            path = resolve_path(value.strip(), self.config)
            if not path.exists() or not path.is_file():
                raise ToolError("image_not_found", f"图片不存在: {path}")
            if path.suffix.lower() not in IMAGE_EXTENSIONS:
                raise ToolError(
                    "unsupported_image",
                    f"仅支持常见图片格式（png/jpg/jpeg/webp/gif/bmp），收到: {path.name}",
                )
            paths.append(str(path))
        return paths

    def _precheck_send(self, wait: bool, force: bool) -> None:
        safety = self.config["safety"]
        failures = int(self.state.data.get("consecutive_failures") or 0)
        if failures >= int(safety["max_consecutive_failures"]) and not force:
            raise ToolError(
                "too_many_consecutive_failures",
                f"已连续失败 {failures} 次，停止投递；排查后可传 force=true 重置失败计数",
                failures=failures,
            )
        if force:
            self.state.reset_failures()
        delivery = self.config["delivery"]
        interval = self._random.uniform(float(delivery["interval_min"]), float(delivery["interval_max"]))
        try:
            self.state.check_send_allowed(interval)
        except SafetyError as exc:
            if wait and exc.code == "send_cooldown" and int(exc.details.get("retry_after_seconds", 0)) <= 600:
                time.sleep(int(exc.details["retry_after_seconds"]) + 1)
                try:
                    self.state.check_send_allowed(interval)
                    return
                except SafetyError as retry_exc:
                    exc = retry_exc
            raise ToolError(exc.code, exc.message, **exc.details) from exc

    def _record_failure(self, job_id: str, url: str, header: dict[str, Any], note: str) -> None:
        self.state.record_delivery(
            job_id,
            status="failed",
            title=str(header.get("title") or ""),
            company=str(header.get("company") or ""),
            url=url,
            note=note,
        )

    # -- job page ----------------------------------------------------------
    @staticmethod
    def _read_job_header(tab: CDPTab) -> dict[str, Any]:
        header = _parse(tab.evaluate(JS_JOB_HEADER))
        return {
            "title": str(header.get("title") or ""),
            "company": str(header.get("company") or ""),
            "salary": str(header.get("salary") or ""),
            "hr": str(header.get("hr") or ""),
        }

    def _click_chat_button(self, tab: CDPTab, attempts: int = 20) -> dict[str, Any]:
        result: dict[str, Any] = {"success": False, "error": "no_chat_button"}
        for attempt in range(max(1, attempts)):
            result = _parse(tab.evaluate(CHAT_BUTTON_SCRIPT))
            if result.get("success"):
                return result
            if attempt + 1 < attempts:
                time.sleep(1.0)
        return result

    def _submit_startchat_greeting(self, tab: CDPTab, greeting: str) -> dict[str, Any]:
        state = _parse(tab.evaluate(JS_STARTCHAT_INPUT_RECT))
        if not state.get("success"):
            return state
        if not tab.click_at(float(state["x"]), float(state["y"])):
            return {"success": False, "error": "startchat_input_focus_failed"}
        if not tab.press_key("SelectAll") or not tab.press_key("Backspace"):
            return {"success": False, "error": "startchat_input_clear_failed"}
        if not tab.type_text(greeting, human=True):
            return {"success": False, "error": "startchat_input_failed"}
        check = _parse(tab.evaluate(JS_STARTCHAT_VALUE))
        if not check.get("success") or self._normalize(check.get("value")) != self._normalize(greeting):
            return {"success": False, "error": "startchat_input_not_filled"}
        submit = _parse(tab.evaluate(JS_STARTCHAT_SUBMIT_RECT))
        if not submit.get("success"):
            return submit
        if not tab.click_at(float(submit["x"]), float(submit["y"])):
            return {"success": False, "error": "startchat_submit_click_failed"}
        return {"success": True, "action": "first_contact_submitted"}

    def _resolve_popup(self, tab: CDPTab, greeting_text: str) -> str:
        self._sleep(2.0, 3.0)
        popup = _parse(tab.evaluate(JS_DETECT_POPUP))
        kind = popup.get("kind") if popup.get("success") else None
        if kind == "preset_greeting":
            confirm = _parse(tab.evaluate(JS_CONFIRM_PRESET))
            if not confirm.get("success"):
                raise ToolError(
                    "preset_greeting_failed",
                    "检测到平台预设招呼语弹窗，但无法确认",
                    detail=confirm,
                )
            self._sleep(1.0, 2.0)
            return "preset_confirmed"
        if kind == "startchat_dialog":
            initial_text = greeting_text or str(self.config["delivery"]["greeting"]).strip()
            if not initial_text:
                raise ToolError(
                    "startchat_greeting_required",
                    "首次沟通需要先发送一条文字消息，请提供 greeting/message",
                )
            submit = self._submit_startchat_greeting(tab, initial_text)
            if not submit.get("success"):
                raise ToolError(
                    "startchat_greeting_failed",
                    "未能提交首次沟通弹窗中的招呼语",
                    detail=submit,
                )
            self._sleep(1.2, 2.2)
            return "first_contact_submitted"
        return "no_popup"

    # -- chat page ---------------------------------------------------------
    def _open_chat_from_list(self, header: dict[str, Any]) -> CDPTab | None:
        hr = str(header.get("hr") or "")
        company = str(header.get("company") or "")
        if not hr and not company:
            return None
        expression = (
            JS_CHAT_LIST_ITEM
            .replace("__HR__", json.dumps(hr, ensure_ascii=False))
            .replace("__COMPANY__", json.dumps(company, ensure_ascii=False))
        )
        list_tab: CDPTab | None = None
        try:
            list_tab = self.browser.open(CHAT_URL, background=True)
            if not list_tab.wait_for_load(20):
                self._sleep(1.0, 2.0)
            list_tab.bring_to_front()
            deadline = time.time() + 20
            while time.time() < deadline:
                found = _parse(list_tab.evaluate(expression))
                if found.get("found"):
                    self._sleep(2.0, 3.0)
                    if list_tab.evaluate("!!document.querySelector('#chat-input')"):
                        return list_tab
                    best = found.get("best") or {}
                    try:
                        list_tab.click_at(float(best.get("x")), float(best.get("y")))
                    except (TypeError, ValueError):
                        pass
                    self._sleep(2.0, 3.0)
                    if list_tab.evaluate("!!document.querySelector('#chat-input')"):
                        return list_tab
                time.sleep(1.0)
                list_tab.navigate(CHAT_URL, wait=True, timeout=20)
                self._sleep(1.0, 2.0)
        except (CDPError, ValueError):
            pass
        if list_tab is not None:
            list_tab.close()
        return None

    def _wait_for_chat(
        self,
        job_tab: CDPTab,
        header: dict[str, Any],
        existing_ids: set[str],
        timeout: float = 20,
    ) -> tuple[CDPTab | None, bool]:
        deadline = time.time() + timeout
        checked_new: set[str] = set()
        while time.time() < deadline:
            state = self._chat_state(job_tab, header)
            if state.get("chat") and state.get("input"):
                return job_tab, False
            try:
                targets = self.browser.chrome.list_targets()
            except CDPError:
                targets = []
            for target in targets:
                target_id = str(target.get("id") or "")
                if not target_id or target_id == job_tab.target_id or target_id in existing_ids or target_id in checked_new:
                    continue
                if "/web/geek/chat" not in str(target.get("url") or ""):
                    continue
                checked_new.add(target_id)
                candidate = self.browser.chrome.tab(target_id)
                candidate_state = self._chat_state(candidate, header)
                if candidate_state.get("chat") and candidate_state.get("input") and candidate_state.get("matches"):
                    return candidate, True
                candidate.close()
            time.sleep(0.5)
        return None, False

    @staticmethod
    def _chat_state(tab: CDPTab, header: dict[str, Any]) -> dict[str, Any]:
        expression = (
            JS_CHAT_READY
            .replace("__COMPANY__", json.dumps(str(header.get("company") or ""), ensure_ascii=False))
            .replace("__TITLE__", json.dumps(str(header.get("title") or ""), ensure_ascii=False))
        )
        return _parse(tab.evaluate(expression))

    def _ensure_greeting(self, tab: CDPTab, greeting: str, contact_action: str) -> tuple[str, str]:
        current = self._message_state(tab, greeting)
        if current == "delivered":
            return "already_present", ""
        if current == "pending":
            state, message = self._wait_message_state(tab, greeting)
            return ("already_present", "") if state == "sent" else (state, message)
        if current == "failed":
            return "failed", "会话中已有的相同消息被标记为发送失败"
        if contact_action == "first_contact_submitted":
            state, message = self._wait_message_state(tab, greeting)
            if state == "sent":
                return "already_present", ""
            if state == "failed":
                return "failed", message
            return "unverified", message
        if contact_action == "no_popup" and self._own_message_count(tab) > 0:
            return "skipped_existing_conversation", ""
        return self._send_chat_text(tab, greeting)

    def _send_chat_text(self, tab: CDPTab, text: str) -> tuple[str, str]:
        expected = self._normalize(text)
        input_state = _parse(tab.evaluate(JS_CHAT_INPUT_VALUE))
        if not input_state.get("success"):
            return "failed", "未找到聊天输入框"
        if not tab.click(
            '#chat-input, .chat-editor [contenteditable="true"], '
            '.chat-input [contenteditable="true"], .chat-editor textarea.input-area'
        ):
            return "failed", "无法聚焦聊天输入框"
        if not tab.press_key("SelectAll") or not tab.press_key("Backspace"):
            return "failed", "无法清空聊天输入框"
        if not tab.type_text(text, human=True):
            return "failed", "无法输入消息"
        check = _parse(tab.evaluate(JS_CHAT_INPUT_VALUE))
        if self._normalize(check.get("value")) != expected:
            fallback = self._js_submit(tab, text)
            if not fallback.get("success"):
                return "failed", "输入框内容与预期不一致"
        else:
            if check.get("disabled"):
                tab.type_text(" ")
                tab.press_key("Backspace")
                check = _parse(tab.evaluate(JS_CHAT_INPUT_VALUE))
            if check.get("disabled"):
                fallback = self._js_submit(tab, text)
                if not fallback.get("success"):
                    return "failed", "发送按钮不可用，且旧版提交入口不可用"
            else:
                clicked = tab.click(CHAT_SEND_SELECTOR) or tab.click_js(CHAT_SEND_SELECTOR)
                if not clicked:
                    fallback = self._js_submit(tab, text)
                    if not fallback.get("success"):
                        return "failed", "发送按钮点击失败"
        state, message = self._wait_message_state(tab, text)
        if state == "sent":
            return "sent", ""
        if state == "failed":
            return "failed", message
        current_value = _parse(tab.evaluate(JS_CHAT_INPUT_VALUE)).get("value")
        if self._normalize(current_value) == expected:
            return "failed", "消息未提交，输入框仍保留原文"
        return "unverified", message or "消息已提交但无法确认送达"

    @staticmethod
    def _js_submit(tab: CDPTab, text: str) -> dict[str, Any]:
        expression = JS_CHAT_SUBMIT_FALLBACK.replace("__TEXT__", json.dumps(text, ensure_ascii=False))
        return _parse(tab.evaluate(expression))

    def _wait_message_state(self, tab: CDPTab, text: str, attempts: int = 24) -> tuple[str, str]:
        state = "missing"
        for _ in range(max(1, attempts)):
            state = self._message_state(tab, text)
            if state == "delivered":
                time.sleep(1.0)
                if self._message_state(tab, text) == "delivered":
                    return "sent", ""
            if state == "failed":
                return "failed", "消息被标记为发送失败"
            time.sleep(0.5)
        if state == "pending":
            return "unverified", "消息仍处于发送中，无法确认最终状态"
        if state == "missing":
            return "unverified", "未在会话中找到刚发送的消息"
        return "unverified", "消息状态未稳定"

    @classmethod
    def _message_state(cls, tab: CDPTab, text: str) -> str:
        expression = JS_MESSAGE_STATE.replace("__TEXT__", json.dumps(text, ensure_ascii=False))
        result = _parse(tab.evaluate(expression))
        if not result.get("success"):
            return "missing"
        return str(result.get("state") or "missing")

    @staticmethod
    def _own_message_count(tab: CDPTab) -> int:
        result = _parse(tab.evaluate(JS_OWN_MESSAGES))
        try:
            return int(result.get("count") or 0)
        except (TypeError, ValueError):
            return 0

    # -- images ------------------------------------------------------------
    def _send_images_in_chat(self, tab: CDPTab, images: list[str]) -> tuple[int, dict[str, Any] | None]:
        sent = 0
        last_error: dict[str, Any] | None = None
        for image in images:
            before = self._image_count(tab)
            error = self._upload_one_image(tab, image, before)
            if error:
                last_error = error
                break
            sent += 1
            self._sleep(1.0, 2.0)
        return sent, last_error

    def _upload_one_image(self, tab: CDPTab, image: str, before: int) -> dict[str, Any] | None:
        if _parse(tab.evaluate(JS_PREVIEW_SEND_BUTTON)).get("found"):
            return {
                "error": "image_preview_pending",
                "message": "聊天窗口已有未发送的图片预览，请先在页面上确认或关闭后再重试",
            }
        probe = _parse(tab.evaluate(JS_PROBE_FILE_INPUT))
        if not probe.get("found"):
            trigger = _parse(tab.evaluate(JS_PROBE_IMAGE_TRIGGER))
            if trigger.get("found"):
                if tab.click_and_choose_files('[data-bosshunter-image-trigger="1"]', [image]):
                    return self._await_image_sent(tab, before)
                self._sleep(0.8, 1.4)
                probe = _parse(tab.evaluate(JS_PROBE_FILE_INPUT))
            if not probe.get("found"):
                return {
                    "error": "image_upload_unavailable",
                    "message": "未找到聊天图片上传控件",
                    "file_inputs": probe.get("candidates", []),
                    "image_triggers": trigger.get("candidates", []),
                }
        if not tab.set_files('[data-bosshunter-upload="1"]', [image]):
            return {
                "error": "image_upload_failed",
                "message": "找到文件输入框但设置图片失败",
                "file_inputs": probe.get("candidates", []),
            }
        return self._await_image_sent(tab, before)

    def _await_image_sent(self, tab: CDPTab, before: int) -> dict[str, Any] | None:
        clicked = False
        # 网络/加速器慢时图片渲染会晚于上传完成，多等一会儿避免误判失败。
        for _ in range(40):
            self._sleep(0.5, 0.8)
            if not clicked:
                preview = _parse(tab.evaluate(JS_PREVIEW_SEND_BUTTON))
                if preview.get("found"):
                    clicked = (
                        tab.click('[data-bosshunter-preview-send="1"]')
                        or tab.click_js('[data-bosshunter-preview-send="1"]')
                    )
                    if clicked:
                        self._sleep(0.8, 1.4)
            if self._image_count(tab) > before:
                return None
        preview_state = _parse(tab.evaluate(JS_IMAGE_PREVIEW_VISIBLE))
        if preview_state.get("visible"):
            return {
                "error": "image_preview_not_sent",
                "message": "图片已选中但预览窗未能发送",
            }
        return {
            "error": "image_send_unverified",
            "message": "图片上传后未在会话中确认出现",
        }

    @staticmethod
    def _image_count(tab: CDPTab) -> int:
        result = _parse(tab.evaluate(JS_OWN_IMAGES))
        try:
            return int(result.get("count") or 0)
        except (TypeError, ValueError):
            return 0

    # -- misc --------------------------------------------------------------
    @staticmethod
    def _normalize(value: Any) -> str:
        return " ".join(str(value or "").replace("\u200b", "").replace("\ufeff", "").split())

    def _sleep(self, minimum: float, maximum: float) -> None:
        time.sleep(self._random.uniform(minimum, maximum))


def deliver_application(
    config: dict[str, Any],
    state: SafetyState,
    job_url: str,
    *,
    greeting: str | None = None,
    images: list[str] | None = None,
    send_greeting: bool = True,
    send_images: bool = True,
    force: bool = False,
    dry_run: bool = False,
    wait: bool = False,
) -> dict[str, Any]:
    return BossDeliverer(config, state).deliver(
        job_url,
        greeting=greeting,
        images=images,
        send_greeting=send_greeting,
        send_images=send_images,
        force=force,
        dry_run=dry_run,
        wait=wait,
    )


def send_images_to_job(
    config: dict[str, Any],
    state: SafetyState,
    job_url: str,
    *,
    images: list[str] | None = None,
    message: str | None = None,
    force: bool = True,
    wait: bool = False,
) -> dict[str, Any]:
    return BossDeliverer(config, state).send_images(
        job_url,
        images=images,
        message=message,
        force=force,
        wait=wait,
    )
