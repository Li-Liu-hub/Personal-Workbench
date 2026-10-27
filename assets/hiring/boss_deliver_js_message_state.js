
(() => {
    const normalize = (value) => String(value || '')
        .replace(/[\u200b-\u200f\ufeff]/g, '')
        .replace(/\s+/g, ' ')
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
