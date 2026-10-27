
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
