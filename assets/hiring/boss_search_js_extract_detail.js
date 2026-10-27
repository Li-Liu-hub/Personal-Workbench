
(() => {
    const info = {};
    info.title = document.querySelector('.info-primary .name h1')?.textContent?.trim()
        || document.querySelector('.name h1')?.textContent?.trim()
        || document.title.split('-')[0]?.trim();
    const salaryEl = document.querySelector('.info-primary .salary') || document.querySelector('.salary');
    info.salary = salaryEl?.textContent?.trim() || '';
    const tagItems = document.querySelectorAll('.info-primary .tag-list span, .job-primary .tag-list span');
    const tagTexts = Array.from(tagItems).map(t => t.textContent.trim()).filter(Boolean);
    info.experience = tagTexts[0] || document.querySelector('.info-primary .text-experiece')?.textContent?.trim() || '';
    info.education = tagTexts[1] || document.querySelector('.info-primary .text-degree')?.textContent?.trim() || '';
    const jdEl = document.querySelector('.job-sec-text');
    info.jd = jdEl ? jdEl.textContent.trim() : '';
    const bodyText = document.body ? document.body.innerText : '';
    info.jd_truncated = /登录查看完整内容|登录后查看/.test(bodyText);
    const pageText = [info.title, info.experience, bodyText.slice(0, 3000)].join(' ');
    info.recruitment_type = /校招|校园招聘|应届|毕业生|管培生/.test(pageText)
        ? 'campus'
        : (/社招|社会招聘/.test(pageText) ? 'experienced' : 'unknown');
    info.company = '';
    const companyLinks = document.querySelectorAll('.sider-company .company-info a');
    for (const link of companyLinks) {
        const text = link.textContent.trim();
        if (text) { info.company = text; break; }
    }
    if (!info.company) {
        const bossMatch = (document.querySelector('.job-boss-info')?.innerText || '').match(/([^\n·]+)·招聘/);
        if (bossMatch) info.company = bossMatch[1].trim();
    }
    if (!info.company) {
        const titleMatch = document.title.match(/_(.+?)招聘/);
        info.company = titleMatch ? titleMatch[1] : '';
    }
    const companyTags = document.querySelectorAll(
        '.sider-company .res-industry-item, .company-info-item, .company-info .company-tag'
    );
    info.company_size = '';
    info.company_industry = '';
    companyTags.forEach(tag => {
        const text = tag.textContent.trim();
        if (!text) return;
        if (text.includes('人')) info.company_size = text;
        else if (!info.company_industry) info.company_industry = text;
    });
    if (!info.company_size) {
        const sidebarText = document.querySelector('.sider-company')?.innerText || '';
        const sizeMatch = sidebarText.match(/(\d+-\d+人|\d+人以上|少于\d+人)/);
        if (sizeMatch) info.company_size = sizeMatch[1];
    }
    if (!info.company_industry) {
        info.company_industry = document.querySelector(
            '.sider-company a[ka="job-detail-brandindustry"], .sider-company .company-info a[href^="/i"]'
        )?.textContent?.trim() || '';
    }
    const bossSection = document.querySelector('.job-boss-info') || document.querySelector('.boss-info-attr')
        || document.querySelector('.boss-info');
    const hrNameEl = bossSection?.querySelector('h2.name, .name, .boss-name');
    info.hr_name = hrNameEl
        ? Array.from(hrNameEl.childNodes)
            .filter(n => n.nodeType === 3)
            .map(n => n.textContent)
            .join('')
            .trim()
        : '';
    const bossAttr = bossSection?.querySelector('.boss-info-attr')?.textContent?.trim() || '';
    info.hr_title = bossAttr ? bossAttr.split('·').pop().trim() : '';
    info.hr_active = bossSection?.querySelector('.boss-online-tag')?.textContent?.trim() || '';
    info.url = location.href;
    info.page_title = document.title;
    return JSON.stringify(info);
})()
