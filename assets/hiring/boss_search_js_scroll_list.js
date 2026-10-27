
(() => {
    const list = document.querySelector('.rec-job-list') || document.querySelector('.job-list-container');
    if (list) list.scrollTop = list.scrollHeight;
    const scroller = document.scrollingElement || document.documentElement;
    if (scroller) scroller.scrollTop = scroller.scrollHeight;
    window.scrollTo(0, scroller ? scroller.scrollHeight : document.body.scrollHeight);
    window.dispatchEvent(new Event('scroll'));
    if (list) list.dispatchEvent(new Event('scroll'));
    return true;
})()
