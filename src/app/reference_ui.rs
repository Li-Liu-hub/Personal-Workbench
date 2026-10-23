use super::*;
use crate::reference::{Entry, Filter, Summary};

#[derive(Default)]
pub(super) struct ReferenceState {
    pub(in crate::app) filter: Filter,
    pub(in crate::app) loaded: Option<Filter>,
    rows: Vec<Summary>,
    facets: Vec<(String, String)>,
    types: Vec<String>,
    tags: Vec<String>,
    total: usize,
    draft_tags: String,
    pub(in crate::app) selected: Option<Entry>,
    markdown: String,
    cache: egui_commonmark::CommonMarkCache,
    pub(in crate::app) draft: Option<Entry>,
    error: String,
}
impl PersonalDeskApp {
    #[cfg(debug_assertions)]
    pub(super) fn preview_reference(&mut self, edit: bool) {
        let mut s = ReferenceState::default();
        s.filter.language = "Java".into();
        s.filter.category = "并发 / 锁与内存模型".into();
        self.select_reference(&mut s, "ref:java-eco:lock-states");
        if edit {
            s.draft_tags = s.selected.as_ref().unwrap().tags.join(", ");
            s.draft = s.selected.clone();
        }
        self.reference = s;
        self.search_query.clear();
    }
    pub(super) fn add_reference(&mut self) {
        if self.reference.draft.is_some() {
            self.notice = "请先保存或取消当前编辑".into();
            return;
        }
        let mut draft = Entry::new();
        if !self.reference.filter.language.is_empty() {
            draft.language = self.reference.filter.language.clone();
        }
        if !self.reference.filter.category.is_empty() {
            draft.category = self.reference.filter.category.clone();
        }
        self.reference.draft_tags.clear();
        self.reference.draft = Some(draft);
        self.reference.error.clear();
    }
    pub(in crate::app) fn select_reference(&mut self, s: &mut ReferenceState, id: &str) {
        match self
            .db
            .as_ref()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.reference_entry(id))
        {
            Ok(Some(item)) => {
                if s.selected.as_ref().is_none_or(|old| old.id != item.id) {
                    s.cache = Default::default();
                }
                s.markdown = item.markdown();
                s.selected = Some(item);
                s.error.clear();
            }
            Ok(None) => {
                s.selected = None;
                s.markdown.clear();
            }
            Err(e) => s.error = e.to_string(),
        }
    }
    fn refresh_reference(&mut self, s: &mut ReferenceState) {
        let result = (|| -> crate::error::Result<_> {
            let db = self.db.as_ref().ok_or(DeskError::UnsupportedFormat)?;
            Ok((
                db.search_reference(&s.filter)?,
                db.reference_facets()?,
                db.reference_metadata(&s.filter.language)?,
                db.reference_count(&s.filter.language)?,
            ))
        })();
        match result {
            Ok((mut rows, facets, (types, tags), total)) => {
                s.total = total;
                s.types = types;
                s.tags = tags;
                rows.sort_by_key(|r| !self.is_favorite("reference", &r.id));
                s.rows = rows;
                s.facets = facets;
                s.loaded = Some(s.filter.clone());
                if s.draft.is_none()
                    && s.selected
                        .as_ref()
                        .is_none_or(|entry| !s.rows.iter().any(|r| r.id == entry.id))
                {
                    if let Some(id) = s.rows.first().map(|r| r.id.clone()) {
                        self.select_reference(s, &id);
                    } else {
                        s.selected = None;
                        s.markdown.clear();
                    }
                }
            }
            Err(e) => s.error = e.to_string(),
        }
    }
    pub(super) fn render_reference(&mut self, ui: &mut egui::Ui) {
        let mut s = std::mem::take(&mut self.reference);
        ui.horizontal(|ui| {
            ui.heading("百科全书");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(primary_button("+ 添加条目")).clicked() {
                    if s.draft.is_some() {
                        s.error = "请先保存或取消当前编辑".into();
                    } else {
                        let mut draft = Entry::new();
                        if !s.filter.language.is_empty() {
                            draft.language = s.filter.language.clone();
                        }
                        if !s.filter.category.is_empty() {
                            draft.category = s.filter.category.clone();
                        }
                        s.draft_tags.clear();
                        s.draft = Some(draft);
                        s.error.clear();
                    }
                }
                if ui
                    .add_enabled(!self.export_busy, egui::Button::new("导出"))
                    .clicked()
                {
                    self.export_dialog_open = true;
                }
            });
        });
        ui.add_space(10.);
        ui.horizontal(|ui| {
            let field = ui.add_sized(
                [ui.available_width() - 68., 32.],
                egui::TextEdit::singleline(&mut self.search_query)
                    .id_salt("reference-search")
                    .hint_text("搜索名称、语法、示例…  Ctrl+K")
                    .margin(egui::Margin::symmetric(10, 7)),
            );
            self.search_field = Some(field.id);
            if ui
                .add_enabled(!self.search_query.is_empty(), egui::Button::new("清空"))
                .clicked()
            {
                self.search_query.clear();
            }
        });
        ui.add_space(6.);
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("reference-language")
                .selected_text(if s.filter.language.is_empty() {
                    "全部语言"
                } else {
                    &s.filter.language
                })
                .width(136.)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(&mut s.filter.language, String::new(), "全部语言")
                        .changed()
                    {
                        s.filter.category.clear();
                        s.filter.tag.clear();
                        s.filter.entry_type.clear();
                    }
                    let mut languages = crate::reference::LANGUAGES
                        .iter()
                        .map(|v| (*v).to_owned())
                        .collect::<Vec<_>>();
                    for (language, _) in &s.facets {
                        if !languages.contains(language) {
                            languages.push(language.clone());
                        }
                    }
                    for language in languages {
                        if ui
                            .selectable_value(&mut s.filter.language, language.clone(), language)
                            .changed()
                        {
                            s.filter.category.clear();
                            s.filter.tag.clear();
                            s.filter.entry_type.clear();
                        }
                    }
                });
            egui::ComboBox::from_id_salt("reference-category")
                .selected_text(if s.filter.category.is_empty() {
                    "全部分类"
                } else {
                    &s.filter.category
                })
                .width(160.)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.filter.category, String::new(), "全部分类");
                    let categories = s
                        .facets
                        .iter()
                        .filter(|(language, _)| {
                            s.filter.language.is_empty() || *language == s.filter.language
                        })
                        .map(|(_, category)| category)
                        .collect::<std::collections::BTreeSet<_>>();
                    for category in categories {
                        ui.selectable_value(&mut s.filter.category, category.clone(), category);
                    }
                });
            egui::ComboBox::from_id_salt("reference-type")
                .width(90.)
                .selected_text(if s.filter.entry_type.is_empty() {
                    "全部类型"
                } else {
                    &s.filter.entry_type
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.filter.entry_type, String::new(), "全部类型");
                    for kind in &s.types {
                        ui.selectable_value(&mut s.filter.entry_type, kind.clone(), kind);
                    }
                });
            egui::ComboBox::from_id_salt("reference-tag")
                .width(120.)
                .selected_text(if s.filter.tag.is_empty() {
                    "全部标签"
                } else {
                    &s.filter.tag
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut s.filter.tag, String::new(), "全部标签");
                    for tag in &s.tags {
                        ui.selectable_value(&mut s.filter.tag, tag.clone(), tag);
                    }
                });
            if ui
                .add_enabled(
                    !s.filter.category.is_empty()
                        || !s.filter.entry_type.is_empty()
                        || !s.filter.tag.is_empty(),
                    egui::Button::new("重置筛选"),
                )
                .clicked()
            {
                s.filter.category.clear();
                s.filter.entry_type.clear();
                s.filter.tag.clear();
            }
        });
        s.filter.query = self.search_query.trim().to_owned();
        if s.loaded.as_ref() != Some(&s.filter) {
            self.refresh_reference(&mut s);
        }
        ui.label(
            RichText::new(format!("匹配 {} / {} 条", s.rows.len(), s.total))
                .small()
                .color(muted()),
        );
        ui.add_space(10.);
        if !s.error.is_empty() {
            ui.colored_label(Color32::from_rgb(190, 68, 68), &s.error);
        }
        let height = ui.available_height().max(160.);
        let list_width = (ui.available_width() * 0.29).clamp(210., 320.);
        let mut practice = None;
        let mut chosen = None;
        let mut deleted = None;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(list_width, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(list_width);
                    ui.spacing_mut().item_spacing.y = 0.;
                    egui::ScrollArea::vertical()
                        .id_salt("reference-list")
                        .auto_shrink([false, false])
                        .max_height(height)
                        .min_scrolled_height(height)
                        .show_rows(ui, 64., s.rows.len(), |ui, range| {
                            for i in range {
                                let item = &s.rows[i];
                                let (rect, response) = ui.allocate_exact_size(
                                    egui::vec2(list_width - 12., 64.),
                                    egui::Sense::click(),
                                );
                                if s.selected.as_ref().is_some_and(|e| e.id == item.id)
                                    || response.hovered()
                                {
                                    ui.painter().rect_filled(
                                        rect.shrink(2.),
                                        8,
                                        sidebar().lerp_to_gamma(accent(), 0.1),
                                    );
                                }
                                reference_label(
                                    ui,
                                    rect.shrink2(egui::vec2(10., 0.))
                                        .translate(egui::vec2(0., -10.)),
                                    &item.title,
                                    14.,
                                    ink(),
                                );
                                reference_label(
                                    ui,
                                    rect.shrink2(egui::vec2(10., 0.))
                                        .translate(egui::vec2(0., 13.)),
                                    &format!("{} · {}", item.language, item.category),
                                    11.,
                                    muted(),
                                );
                                if response.clone().on_hover_text(&item.summary).clicked() {
                                    chosen = Some(item.id.clone());
                                }
                                response.context_menu(|ui| {
                                    if ui.button("删除条目").clicked() {
                                        deleted = Some(item.id.clone());
                                        ui.close();
                                    }
                                });
                            }
                        });
                },
            );
            ui.separator();
            let width = ui.available_width();
            ui.allocate_ui_with_layout(
                egui::vec2(width, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(width);
                    let mut save = false;
                    let mut cancel = false;
                    let mut edit = false;
                    if let Some(draft) = s.draft.as_ref() {
                        ui.horizontal(|ui| {
                            ui.label(if draft.revision == 0 {
                                "添加条目"
                            } else {
                                "编辑条目"
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    cancel = ui.button("取消").clicked();
                                    save = ui.add(primary_button("保存")).clicked();
                                },
                            );
                        });
                    } else if let Some(item) = s.selected.as_ref() {
                        ui.horizontal_wrapped(|ui| {
                            if ui.add(primary_button("练习")).clicked() {
                                practice = Some(item.clone());
                            }
                            if ui.button("复制示例").clicked() {
                                ui.ctx().copy_text(item.example.clone());
                                self.notice = "示例已复制".into();
                            }
                            edit = ui.button("编辑").clicked();
                            if ui
                                .button(if self.is_favorite("reference", &item.id) {
                                    "★ 已收藏"
                                } else {
                                    "☆ 收藏"
                                })
                                .clicked()
                            {
                                self.toggle_favorite("reference", &item.id);
                                s.loaded = None;
                            }
                        });
                    }
                    ui.add_space(8.);
                    egui::ScrollArea::vertical()
                        .id_salt((
                            "reference-detail",
                            s.draft.as_ref().or(s.selected.as_ref()).map(|e| &e.id),
                        ))
                        .max_height(ui.available_height())
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if let Some(draft) = s.draft.as_mut() {
                                reference_field(ui, "标题", &mut draft.title, 1, false);
                                ui.columns(2, |cols| {
                                    reference_field(
                                        &mut cols[0],
                                        "语言",
                                        &mut draft.language,
                                        1,
                                        false,
                                    );
                                    reference_field(
                                        &mut cols[1],
                                        "分类",
                                        &mut draft.category,
                                        1,
                                        false,
                                    );
                                });
                                reference_field(ui, "类型", &mut draft.entry_type, 1, false);
                                reference_field(
                                    ui,
                                    "标签（逗号分隔）",
                                    &mut s.draft_tags,
                                    1,
                                    false,
                                );
                                draft.tags = s
                                    .draft_tags
                                    .split([',', '，'])
                                    .map(str::trim)
                                    .filter(|s| !s.is_empty())
                                    .map(str::to_owned)
                                    .collect();
                                reference_field(ui, "说明", &mut draft.summary, 2, false);
                                reference_field(ui, "语法", &mut draft.syntax, 3, true);
                                reference_field(ui, "示例", &mut draft.example, 6, true);
                                reference_field(ui, "要点", &mut draft.notes, 3, false);
                                reference_field(ui, "参考地址", &mut draft.source, 1, false);
                            } else if let Some(item) = s.selected.as_ref() {
                                ui.heading(&item.title);
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(&item.entry_type).color(muted()));
                                    for tag in &item.tags {
                                        if ui.small_button(tag).clicked() {
                                            s.filter.tag = tag.clone();
                                        }
                                    }
                                });
                                ui.add_space(10.);
                                egui_commonmark::CommonMarkViewer::new().show(
                                    ui,
                                    &mut s.cache,
                                    &s.markdown,
                                );
                                if !item.source.is_empty() {
                                    ui.add_space(12.);
                                    ui.hyperlink_to("参考文档", &item.source);
                                }
                            } else {
                                ui.label("没有匹配条目");
                            }
                        });
                    if edit {
                        s.draft_tags = s
                            .selected
                            .as_ref()
                            .map(|e| e.tags.join(", "))
                            .unwrap_or_default();
                        s.draft = s.selected.clone();
                    }
                    if cancel {
                        s.draft = None;
                        s.error.clear();
                        s.loaded = None;
                    }
                    if save && let Some(draft) = s.draft.as_ref() {
                        match self
                            .db
                            .as_mut()
                            .ok_or(DeskError::UnsupportedFormat)
                            .and_then(|db| db.save_reference(draft))
                        {
                            Ok(item) => {
                                s.markdown = item.markdown();
                                s.selected = Some(item);
                                s.draft = None;
                                s.error.clear();
                                // A newly chosen classification must not hide the item just saved.
                                s.filter.entry_type.clear();
                                s.filter.tag.clear();
                                s.filter.language.clear();
                                s.filter.category.clear();
                                self.search_query.clear();
                                s.filter.query.clear();
                                s.loaded = None;
                                self.notice = "条目已保存".into();
                            }
                            Err(e) => s.error = e.to_string(),
                        }
                    }
                },
            );
        });
        if let Some(id) = chosen {
            if s.draft.is_some() {
                s.error = "请先保存或取消当前编辑".into();
            } else {
                self.select_reference(&mut s, &id);
            }
        }
        if let Some(id) = deleted {
            let result = self
                .db
                .as_mut()
                .ok_or(DeskError::UnsupportedFormat)
                .and_then(|db| {
                    let item = db
                        .reference_entry(&id)?
                        .ok_or(DeskError::ConcurrentModification)?;
                    db.delete_reference(&item)
                });
            match result {
                Ok(()) => {
                    if s.draft.as_ref().is_some_and(|e| e.id == id) {
                        s.draft = None;
                    }
                    s.loaded = None;
                    self.notice = "条目已删除".into();
                }
                Err(e) => s.error = e.to_string(),
            }
        }
        self.reference = s;
        if let Some(item) = practice {
            self.practice_knowledge(&item);
        }
        self.handle_workspace_shortcuts(&ui.ctx().clone());
    }
}
fn reference_label(ui: &egui::Ui, rect: egui::Rect, text: &str, size: f32, color: Color32) {
    let mut job = egui::text::LayoutJob::simple(
        text.into(),
        egui::FontId::proportional(size),
        color,
        rect.width(),
    );
    job.wrap.max_rows = 1;
    let galley = ui.painter().layout_job(job);
    ui.painter()
        .with_clip_rect(rect.intersect(ui.clip_rect()))
        .galley(
            egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.),
            galley,
            color,
        );
}
fn reference_field(ui: &mut egui::Ui, label: &str, value: &mut String, rows: usize, code: bool) {
    ui.label(label);
    let field = if rows == 1 {
        egui::TextEdit::singleline(value)
    } else {
        egui::TextEdit::multiline(value).desired_rows(rows)
    };
    ui.add(
        if code { field.code_editor() } else { field }
            .id(egui::Id::unique(("reference-field", label)))
            .desired_width(f32::INFINITY),
    );
}

#[cfg(test)]
mod tests;
