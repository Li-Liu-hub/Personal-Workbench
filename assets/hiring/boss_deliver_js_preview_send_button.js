
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
