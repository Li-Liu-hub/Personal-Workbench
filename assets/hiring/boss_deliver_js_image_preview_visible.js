
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
