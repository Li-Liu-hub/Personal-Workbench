
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
            || (typeof body === 'string' && /<img|image\//i.test(body));
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
