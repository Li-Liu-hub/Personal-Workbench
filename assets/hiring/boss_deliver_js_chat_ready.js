
(() => {
    const normalize = (value) => String(value || '').replace(/\s+/g, '').toLowerCase();
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
