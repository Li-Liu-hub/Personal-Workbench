use super::*;
use crate::jobhunt::Site;
use std::time::Instant;

#[derive(Default)]
pub(super) struct JobHuntState {
    sites: Vec<Site>,
    loaded: Option<Instant>,
    draft: Option<Site>,
    deleting: Option<Site>,
    error: String,
}

impl JobHuntState {
    pub(super) fn dialog_open(&self) -> bool {
        self.draft.is_some() || self.deleting.is_some()
    }
}

impl PersonalDeskApp {
    pub(super) fn new_jobhunt_site(&mut self) {
        self.jobhunt.draft = Some(Site::new());
        self.jobhunt.error.clear();
    }

    fn refresh_jobhunt(&mut self) {
        match self
            .db
            .as_ref()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.jobhunt_sites())
        {
            Ok(sites) => {
                self.jobhunt.sites = sites;
                self.jobhunt.error.clear();
            }
            Err(error) => self.jobhunt.error = format!("读取公司网站失败：{error}"),
        }
        self.jobhunt.loaded = Some(Instant::now());
    }

    pub(super) fn render_jobhunt(&mut self, ui: &mut egui::Ui) {
        if self
            .jobhunt
            .loaded
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(3))
        {
            self.refresh_jobhunt();
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{} 条公司网站记录", self.jobhunt.sites.len()));
            if ui.button("刷新").clicked() {
                self.refresh_jobhunt();
            }
        });
        ui.small(
            "此处只保存你手动添加的公司官网链接。校招搜索、赛事搜索和投递流程已拆分到独立项目。",
        );
        if !self.jobhunt.error.is_empty() {
            ui.colored_label(Color32::from_rgb(184, 65, 71), &self.jobhunt.error);
        }

        let query = self.search_query.to_lowercase();
        let sites: Vec<_> = self
            .jobhunt
            .sites
            .iter()
            .filter(|site| {
                query.is_empty()
                    || format!(
                        "{} {} {} {} {}",
                        site.company, site.position, site.location, site.url, site.notes
                    )
                    .to_lowercase()
                    .contains(&query)
            })
            .cloned()
            .collect();
        let no_sites = sites.is_empty();
        let grid = self.view_mode() == ViewMode::Grid;
        let width = if grid {
            ((ui.available_width() - 14.) / 2.).max(280.)
        } else {
            ui.available_width()
        };
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true),
            |ui| {
                for site in sites {
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 0.),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            egui::Frame::new()
                                .fill(surface())
                                .corner_radius(10)
                                .inner_margin(14)
                                .show(ui, |ui| {
                                    ui.set_width((width - 28.).max(240.));
                                    ui.label(RichText::new(&site.company).strong());
                                    ui.label(if site.position.is_empty() {
                                        "公司招聘网站"
                                    } else {
                                        &site.position
                                    });
                                    ui.small(&site.location);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&site.url).small().color(muted()),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(&site.url);
                                    if !site.notes.is_empty() {
                                        ui.label(&site.notes);
                                    }
                                    ui.horizontal_wrapped(|ui| {
                                        ui.hyperlink_to("打开网站", &site.url);
                                        if ui.small_button("编辑").clicked() {
                                            self.jobhunt.draft = Some(site.clone());
                                        }
                                        if ui.small_button("删除").clicked() {
                                            self.jobhunt.deleting = Some(site.clone());
                                        }
                                    });
                                });
                        },
                    );
                }
            },
        );
        if no_sites {
            ui.label("暂无匹配网站。点击“添加网站”保存公司官网地址。");
        }
    }

    pub(super) fn render_jobhunt_dialogs(&mut self, ctx: &egui::Context) {
        if self.jobhunt.draft.is_some() {
            let mut save = false;
            let draft = self.jobhunt.draft.as_mut().unwrap();
            let dialog = centered_dialog(ctx, "jobhunt-editor", "公司网站", "", 600., |ui| {
                for (label, value) in [
                    ("公司", &mut draft.company),
                    ("岗位", &mut draft.position),
                    ("网址", &mut draft.url),
                    ("城市", &mut draft.location),
                ] {
                    ui.label(label);
                    ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
                }
                ui.label("备注");
                ui.add(
                    egui::TextEdit::multiline(&mut draft.notes)
                        .desired_width(f32::INFINITY)
                        .desired_rows(4),
                );
                if !self.jobhunt.error.is_empty() {
                    ui.label(&self.jobhunt.error);
                }
                save = ui.add(primary_button("保存网站记录")).clicked();
            });
            if save {
                let result = self
                    .db
                    .as_mut()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| db.save_jobhunt_site(self.jobhunt.draft.as_mut().unwrap()));
                match result {
                    Ok(()) => {
                        self.jobhunt.draft = None;
                        self.refresh_jobhunt();
                    }
                    Err(error) => self.jobhunt.error = error.to_string(),
                }
            } else if dialog_closed(&dialog) {
                self.jobhunt.draft = None;
            }
        }
        if let Some(site) = self.jobhunt.deleting.clone() {
            let mut confirm = false;
            let dialog =
                centered_dialog(ctx, "jobhunt-delete", "删除网站记录", "", 480., |ui| {
                    ui.label(format!("确认删除 {} 的网站记录？", site.company));
                    confirm = ui.button("确认删除记录").clicked();
                    if !self.jobhunt.error.is_empty() {
                        ui.label(&self.jobhunt.error);
                    }
                });
            if confirm {
                match self
                    .db
                    .as_mut()
                    .ok_or(DeskError::UnsupportedFormat)
                    .and_then(|db| db.delete_jobhunt_site(&site))
                {
                    Ok(()) => {
                        self.jobhunt.deleting = None;
                        self.refresh_jobhunt();
                    }
                    Err(error) => self.jobhunt.error = error.to_string(),
                }
            } else if dialog_closed(&dialog) {
                self.jobhunt.deleting = None;
            }
        }
    }
}
