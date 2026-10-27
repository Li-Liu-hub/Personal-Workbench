
(() => {
    const path = location.pathname || '';
    const text = document.body ? document.body.innerText.slice(0, 8000) : '';
    const hasChatUi = Boolean(document.querySelector(
        '#chat-input, .chat-editor, .chat-record, '
        + '.user-nav .nav-figure, .user-nav .figure, .user-nav img'
    ));
    const loginForm = Boolean(document.querySelector(
        '.login-form, .login-dialog, .sign-in, form[class*="login"], [class*="login-form"]'
    ));
    const loginUrl = /\/web\/user\/(?:login)?(?:\?|$)/i.test(path) || /\/login(?:\?|$)/i.test(path);
    const loginText = /扫码登录|短信登录|密码登录|登录后查看|请先登录|登录状态已失效|当前登录状态/.test(text);
    return JSON.stringify({
        logged_in: !loginUrl && !loginForm && !loginText && hasChatUi,
        login_url: loginUrl,
        login_form: loginForm,
        has_chat_ui: hasChatUi,
        login_text: loginText,
        path: path,
        title: document.title || ''
    });
})()
