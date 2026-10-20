use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use std::{cell::RefCell, num::NonZeroIsize, path::PathBuf, rc::Rc};
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
                    });
                    self.selected = self.tabs.len() - 1;
                }
                self.input = url;
                self.error.clear();
            }
            Err(error) => self.error = error,
        }
    }

    #[cfg(debug_assertions)]
    pub fn debug_probe(&self, path: PathBuf) {
        let Some(view) = self
            .tabs
            .get(self.selected)
            .and_then(|tab| tab.view.as_ref())
        else {
            return;
        };
        if !view
            .url()
            .is_ok_and(|url| url.starts_with("http://127.0.0.1:"))
        {
            return;
        }
        let _=view.evaluate_script_with_callback("({ready:document.readyState==='complete',title:document.title,stored:localStorage.getItem('desk-fixture'),cookie:document.cookie.includes('desk-fixture=persisted')})",move|value|{
            if let Ok(json)=serde_json::from_str::<serde_json::Value>(&value)&& json["ready"]==true&&json["title"]=="PersonalDesk Browser Test"{let _=std::fs::write(&path,serde_json::to_vec(&json).unwrap_or_default());}
        });
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
    pub fn script(&mut self, script: &str) {
        if let Some(view) = self.tabs.get(self.selected).and_then(|t| t.view.as_ref())
            && let Err(e) = view.evaluate_script(script)
        {
            self.error = e.to_string();
        }
    }
    pub fn close(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.tabs.remove(index);
            self.selected = self.selected.min(self.tabs.len().saturating_sub(1));
            if let Some(tab) = self.tabs.get(self.selected) {
                self.input = tab.address.clone();
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
        for (index, tab) in self.tabs.iter_mut().enumerate() {
            if let Some(view) = &tab.view {
                let _ = view.set_visible(visible && index == self.selected);
            }
        }
        if !visible || parent == 0 {
            return;
        }
        let Some(area) = area else {
            return;
        };
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
            let pending = self.pending.clone();
            let result = wry::WebViewBuilder::new_with_web_context(&mut self.context)
                .with_url(&tab.address)
                .with_bounds(bounds)
                .with_visible(true)
                .with_new_window_req_handler(move |url, _| {
                    pending.borrow_mut().push(url);
                    wry::NewWindowResponse::Deny
                })
                .build_as_child(&Host(parent));
            match result {
                Ok(view) => tab.view = Some(view),
                Err(error) => {
                    self.error = format!("网页无法启动：{error}");
                    return;
                }
            }
        }
        if let Some(view) = &tab.view {
            let _ = view.set_bounds(bounds);
            if let Ok(url) = view.url() {
                if self.input == tab.address {
                    self.input = url.clone();
                }
                tab.address = url;
            }
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
