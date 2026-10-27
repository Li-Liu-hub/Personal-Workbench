
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
