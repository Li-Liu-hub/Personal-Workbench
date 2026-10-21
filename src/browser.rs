use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use std::{cell::RefCell, num::NonZeroIsize, path::PathBuf, rc::Rc};
use wry::WebViewExtWindows;
#[derive(Default)]
pub struct PageState {
    pub loading: bool,
    pub error: String,
    title: String,
    address: String,
}
pub struct Host(pub usize);
impl HasWindowHandle for Host {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let handle = NonZeroIsize::new(self.0 as isize).ok_or(HandleError::Unavailable)?;
        Ok(unsafe {
            WindowHandle::borrow_raw(RawWindowHandle::Win32(Win32WindowHandle::new(handle)))
        })
    }
}
pub struct Tab {
    pub id: String,
    pub title: String,
    pub address: String,
    pub view: Option<wry::WebView>,
    state: Rc<RefCell<PageState>>,
    visible: bool,
    bounds: Option<(i32, i32, u32, u32)>,
}
pub struct Browser {
    pub tabs: Vec<Tab>,
    pub selected: usize,
    pub input: String,
    pub error: String,
    context: wry::WebContext,
    pending: Rc<RefCell<Vec<String>>>,
}
impl Browser {
    pub fn new(profile: PathBuf) -> Self {
        Self {
            tabs: vec![],
            selected: 0,
            input: String::new(),
            error: String::new(),
            context: wry::WebContext::new(Some(profile)),
            pending: Rc::new(RefCell::new(vec![])),
        }
    }
    pub fn open(&mut self, url: &str) {
        match address(url) {
            Ok(url) => {
                if let Some(index) = self.tabs.iter().position(|tab| tab.address == url) {
                    self.selected = index;
                } else {
                    self.tabs.push(Tab {
                        id: uuid::Uuid::new_v4().to_string(),
                        title: url::Url::parse(&url)
                            .ok()
                            .and_then(|u| u.host_str().map(str::to_owned))
                            .unwrap_or_else(|| "网页".into()),
                        address: url.clone(),
                        view: None,
                        state: Rc::new(RefCell::new(PageState {
                            loading: true,
                            ..Default::default()
                        })),
                        visible: false,
                        bounds: None,
                    });
                    self.selected = self.tabs.len() - 1;
                }
                self.input = url;
                self.error.clear();
            }
            Err(error) => self.error = error,
        }
    }

    pub fn navigate(&mut self) {
        match address(&self.input) {
            Ok(url) => {
                if self.tabs.is_empty() {
                    self.open(&url);
                    return;
                }
                let tab = &mut self.tabs[self.selected];
                if let Some(view) = &tab.view
                    && let Err(e) = view.load_url(&url)
                {
                    self.error = e.to_string();
                    return;
                }
                tab.address = url;
                self.input = tab.address.clone();
            }
            Err(error) => self.error = error,
        }
    }

    pub fn close(&mut self, index: usize) {
        if index < self.tabs.len() {
            if let Some(view) = self.tabs[index].view.as_ref()
                && self.tabs[index].visible
            {
                let _ = view.focus_parent();
            }
            self.tabs.remove(index);
            if index < self.selected {
                self.selected -= 1;
            }
            self.selected = self.selected.min(self.tabs.len().saturating_sub(1));
            if let Some(tab) = self.tabs.get(self.selected) {
                self.input = tab.address.clone();
            }
        }
    }
    pub fn loading(&self) -> bool {
        self.tabs
            .get(self.selected)
            .is_some_and(|tab| tab.state.borrow().loading)
    }
    pub fn page_error(&self) -> String {
        self.tabs
            .get(self.selected)
            .map(|tab| tab.state.borrow().error.clone())
            .unwrap_or_default()
    }
    pub fn back(&mut self) {
        if let Some(view) = self
            .tabs
            .get(self.selected)
            .and_then(|tab| tab.view.as_ref())
            && let Err(error) = view.go_back()
        {
            self.error = error.to_string();
        }
    }
    pub fn forward(&mut self) {
        if let Some(view) = self
            .tabs
            .get(self.selected)
            .and_then(|tab| tab.view.as_ref())
            && let Err(error) = view.go_forward()
        {
            self.error = error.to_string();
        }
    }
    pub fn reload(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.selected) {
            tab.state.borrow_mut().error.clear();
            tab.state.borrow_mut().loading = true;
            if let Some(view) = &tab.view
                && let Err(error) = view.reload()
            {
                self.error = error.to_string();
            }
        }
    }
    pub fn sync(
        &mut self,
        parent: usize,
        area: Option<eframe::egui::Rect>,
        scale: f32,
        visible: bool,
    ) {
        let pending = self.pending.borrow_mut().drain(..).collect::<Vec<_>>();
        for url in pending {
            self.open(&url);
        }
        let mut focus_selected = false;
        for (index, tab) in self.tabs.iter_mut().enumerate() {
            let show = visible && index == self.selected;
            if let Some(view) = &tab.view
                && tab.visible != show
            {
                if !show {
                    let _ = view.focus_parent();
                }
                if let Err(error) = view.set_visible(show) {
                    self.error = error.to_string();
                }
                if show {
                    focus_selected = true;
                }
                tab.visible = show;
            }
            let state = tab.state.borrow();
            if !state.title.is_empty() {
                tab.title = state.title.clone();
            }
            if !state.address.is_empty() && state.address != tab.address {
                if index == self.selected && self.input == tab.address {
                    self.input = state.address.clone();
                }
                tab.address = state.address.clone();
            }
        }
        if !visible || parent == 0 {
            return;
        }
        let Some(area) = area else {
            return;
        };
        let coordinates = (
            (area.left() * scale).round() as i32,
            (area.top() * scale).round() as i32,
            (area.width() * scale).max(1.0).round() as u32,
            (area.height() * scale).max(1.0).round() as u32,
        );
        let bounds = wry::Rect {
            position: wry::dpi::PhysicalPosition::new(
                (area.left() * scale).round() as i32,
                (area.top() * scale).round() as i32,
            )
            .into(),
            size: wry::dpi::PhysicalSize::new(
                (area.width() * scale).max(1.0).round() as u32,
                (area.height() * scale).max(1.0).round() as u32,
            )
            .into(),
        };
        let Some(tab) = self.tabs.get_mut(self.selected) else {
            return;
        };
        if tab.view.is_none() {
            // The OpenGL host must clip native children rather than painting over them.
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::*;
                let host = parent as windows_sys::Win32::Foundation::HWND;
                let style = GetWindowLongPtrW(host, GWL_STYLE);
                SetWindowLongPtrW(host, GWL_STYLE, style | WS_CLIPCHILDREN as isize);
            }
            let pending = self.pending.clone();
            let state = tab.state.clone();
            let title_state = tab.state.clone();
            let navigation_state = tab.state.clone();
            let result = wry::WebViewBuilder::new_with_web_context(&mut self.context)
                .with_url(&tab.address)
                .with_bounds(bounds)
                .with_visible(true)
                .with_focused(false)
                .with_navigation_handler(move |url| {
                    if url.starts_with("http://") || url.starts_with("https://") {
                        let mut state = navigation_state.borrow_mut();
                        state.loading = true;
                        state.error.clear();
                        state.address = url;
                    }
                    true
                })
                .with_document_title_changed_handler(move |title| {
                    title_state.borrow_mut().title = title
                })
                .with_on_page_load_handler(move |event, url| {
                    if url == "about:blank" {
                        return;
                    }
                    let mut state = state.borrow_mut();
                    state.address = url;
                    state.loading = matches!(event, wry::PageLoadEvent::Started);
                    if state.loading {
                        state.error.clear();
                    }
                })
                .with_new_window_req_handler(move |url, _| {
                    pending.borrow_mut().push(url);
                    wry::NewWindowResponse::Deny
                })
                .build_as_child(&Host(parent));
            match result {
                Ok(view) => {
                    let state = tab.state.clone();
                    let mut token = 0;
                    let handler = webview2_com::NavigationCompletedEventHandler::create(Box::new(
                        move |webview, args| {
                            if let Some(webview) = webview {
                                unsafe {
                                    let mut uri = Default::default();
                                    webview.Source(&mut uri)?;
                                    if webview2_com::take_pwstr(uri) == "about:blank" {
                                        return Ok(());
                                    }
                                }
                            }
                            if let Some(args) = args {
                                unsafe {
                                    let mut success = Default::default();
                                    args.IsSuccess(&mut success)?;
                                    let mut state = state.borrow_mut();
                                    state.loading = false;
                                    if !success.as_bool() {
                                        let mut status = Default::default();
                                        args.WebErrorStatus(&mut status)?;
                                        state.error =
                                            format!("网页加载失败（{}），可刷新重试", status.0);
                                    }
                                }
                            }
                            Ok(())
                        },
                    ));
                    if let Err(error) =
                        unsafe { view.webview().add_NavigationCompleted(&handler, &mut token) }
                    {
                        self.error = error.to_string();
                    }
                    tab.visible = true;
                    tab.bounds = Some(coordinates);
                    let _ = view.focus();
                    tab.view = Some(view);
                }
                Err(error) => {
                    self.error = format!("网页无法启动：{error}");
                    return;
                }
            }
        }
        if let Some(view) = &tab.view
            && tab.bounds != Some(coordinates)
        {
            if let Err(error) = view.set_bounds(bounds) {
                self.error = error.to_string();
            }
            tab.bounds = Some(coordinates);
        }
        // Hide every previous view before focusing the selected one. Otherwise a
        // later tab's focus_parent() would undo this tab's focus in the same frame.
        if focus_selected && let Some(view) = &tab.view {
            let _ = view.focus();
        }
    }
}
pub fn address(value: &str) -> Result<String, String> {
    let value = value.trim();
    let full = if value.contains("://") {
        value.to_owned()
    } else {
        format!("https://{value}")
    };
    let parsed = url::Url::parse(&full).map_err(|_| "请输入网站地址".to_owned())?;
    if !matches!(parsed.scheme(), "https" | "http") || parsed.host_str().is_none() {
        return Err("请输入 http 或 https 网站地址".into());
    }
    Ok(parsed.to_string())
}
