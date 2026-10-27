
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
        const looksImage = !accept || /image|\.(jpg|jpeg|png|gif|webp|bmp)/.test(accept);
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
