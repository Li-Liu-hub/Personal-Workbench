use super::hierarchy::{BookDrag, LibraryDrop};
use super::*;
use crate::library::Book;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct BookNavigation {
    book_id: String,
    query: String,
    revision: Option<u64>,
    books: Vec<Book>,
    pages: HashMap<String, Vec<Page>>,
    pub(super) expanded: HashSet<String>,
    error: String,
}

enum DirectoryAction {
    Drop(LibraryDrop),
    Parent,
    Create,
    Open(String),
    Delete(String, String),
}

#[derive(Default)]
struct DirectoryActions {
    drop: Option<LibraryDrop>,
    select: Option<String>,
    remove: Option<(String, String)>,
}

// A page switch must happen after the paper has rendered, otherwise clearing the
// current page mid-frame leaves the editor pane without a page to draw.
fn current_page<'a>(saved: &'a Page, draft: Option<&'a Page>) -> &'a Page {
    draft.filter(|draft| draft.id == saved.id).unwrap_or(saved)
}

fn subtree_matches(
    books: &[Book],
    pages: &HashMap<String, Vec<Page>>,
    id: &str,
    query: &str,
    draft: Option<&Page>,
) -> bool {
    pages.get(id).is_some_and(|pages| {
        pages
            .iter()
            .any(|page| matches(current_page(page, draft), query))
    }) || books
        .iter()
        .filter(|book| book.parent_id.as_deref() == Some(id))
        .any(|child| subtree_matches(books, pages, &child.id, query, draft))
}

fn page_row(
    ui: &mut egui::Ui,
    page: &Page,
    draft: Option<&Page>,
    query: &str,
    actions: &mut DirectoryActions,
) {
    let active = draft.is_some_and(|draft| draft.id == page.id);
    let response = ui
        .add_sized(
            [ui.available_width(), 32.],
            egui::Button::selectable(active, &page.title)
                .sense(egui::Sense::click_and_drag())
                .wrap_mode(egui::TextWrapMode::Truncate),
        )
        .on_hover_text(&page.title);
    response.dnd_set_drag_payload(WritingDrag(page.id.clone()));
    if let Some(action) = hierarchy::page_drop(ui, &response, &page.id) {
        actions.drop = Some(action);
    }
    if response.clicked() && !active {
        actions.select = Some(page.id.clone());
    }
    response.context_menu(|ui| {
        if ui.button("删除页面").clicked() {
            actions.remove = Some((page.id.clone(), page.title.clone()));
            ui.close();
        }
    });
    ui.add(
        egui::Label::new(
            RichText::new(excerpt(&page.body, query))
                .small()
                .color(muted()),
        )
        .truncate(),
    );
    ui.add_space(6.);
}

fn child_books(
    ui: &mut egui::Ui,
    parent: &str,
    contents: (&[Book], &HashMap<String, Vec<Page>>),
    expanded: &mut HashSet<String>,
    query: &str,
    draft: Option<&Page>,
    actions: &mut DirectoryActions,
) {
    let (books, pages) = contents;
    let searching = !query.trim().is_empty();
    for child in books
        .iter()
        .filter(|book| book.parent_id.as_deref() == Some(parent))
    {
        if searching && !subtree_matches(books, pages, &child.id, query, draft) {
            continue;
        }
        let mut open = searching || expanded.contains(&child.id);
        let arrow = if open { "▾" } else { "▸" };
        let response = ui.add_sized(
            [ui.available_width(), 32.],
            egui::Button::new(format!("{arrow} 图书 · {}", child.title))
                .sense(egui::Sense::click_and_drag())
                .wrap_mode(egui::TextWrapMode::Truncate),
        );
        response.dnd_set_drag_payload(BookDrag(child.id.clone()));
        if let Some(action) = hierarchy::book_drop(ui, &response, &child.id) {
            actions.drop = Some(action);
        }
        if response.clicked() && !searching {
            open = !open;
            if open {
                expanded.insert(child.id.clone());
            } else {
                expanded.remove(&child.id);
            }
        }
        if open {
            ui.indent(("book-directory-child", &child.id), |ui| {
                let child_pages = pages.get(&child.id).map(Vec::as_slice).unwrap_or(&[]);
                let mut shown = 0;
                for page in child_pages {
                    let page = current_page(page, draft);
                    if matches(page, query) {
                        page_row(ui, page, draft, query, actions);
                        shown += 1;
                    }
                }
                let has_children = books
                    .iter()
                    .any(|book| book.parent_id.as_deref() == Some(child.id.as_str()));
                if shown == 0 && !has_children {
                    ui.label(RichText::new("暂无页面").small().color(muted()));
                    ui.add_space(6.);
                }
                child_books(
                    ui,
                    &child.id,
                    (books, pages),
                    expanded,
                    query,
                    draft,
                    actions,
                );
            });
        }
    }
}

// Search the full content, including the current recovery draft, rather than just its preview.
fn matches(page: &Page, query: &str) -> bool {
    if query.trim().is_empty() {
        return true;
    }
    let haystack = format!("{}\n{}", page.title, page.body).to_lowercase();
    query
        .split_whitespace()
        .all(|word| haystack.contains(&word.to_lowercase()))
}

fn excerpt(body: &str, query: &str) -> String {
    let text = body.replace(['\n', '\r'], " ");
    let lowered = text.to_lowercase();
    let start = query
        .split_whitespace()
        .find_map(|word| lowered.find(&word.to_lowercase()))
        // Convert byte offsets to characters so Chinese excerpts never split UTF-8.
        .map(|at| lowered[..at].chars().count().saturating_sub(12))
        .unwrap_or(0);
    let mut value = text.chars().skip(start).take(56).collect::<String>();
    if start > 0 {
        value.insert(0, '…');
    }
    if text.chars().count() > start + 56 {
        value.push('…');
    }
    value
}

impl PersonalDeskApp {
    pub(super) fn render_book_workspace(
        &mut self,
        ui: &mut egui::Ui,
        book: &Book,
        format: Option<&str>,
    ) {
        const GAP: f32 = 12.;
        let area = ui.available_rect_before_wrap();
        let default = (area.width() * 0.25).clamp(210., 270.);
        let maximum = (area.width() - 320. - GAP).clamp(150., 640.);
        let preferred = *self.writing.book_directory_width.get_or_insert_with(|| {
            self.db
                .as_ref()
                .and_then(|db| db.settings().ok())
                .and_then(|settings| {
                    settings
                        .get("writing.directory.width")
                        .and_then(|v| v.parse::<f32>().ok())
                })
                .filter(|width| width.is_finite() && (150. ..=640.).contains(width))
                .unwrap_or(default)
        });
        let mut width = preferred.clamp(150., maximum);
        let handle_rect = |width| {
            egui::Rect::from_min_size(
                egui::pos2(area.left() + width, area.top()),
                egui::vec2(GAP, area.height()),
            )
        };
        let response = ui
            .interact(
                handle_rect(width),
                egui::Id::unique("book-directory-resize"),
                egui::Sense::click_and_drag(),
            )
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
            .on_hover_text("拖动调整目录宽度；双击恢复默认宽度");
        if (response.dragged() || response.drag_stopped())
            && let Some(pointer) = response.interact_pointer_pos()
        {
            width = (pointer.x - area.left() - GAP / 2.).clamp(150., maximum);
            self.writing.book_directory_width = Some(width);
        }
        if response.double_clicked() {
            width = default.clamp(150., maximum);
            self.writing.book_directory_width = Some(width);
        }
        if (response.drag_stopped() || response.double_clicked())
            && let Some(db) = self.db.as_mut()
            && let Err(error) = db.set_setting("writing.directory.width", &width.to_string())
        {
            self.notice = format!("目录宽度未保存：{error}");
        }
        let handle = handle_rect(width);
        let color = if response.hovered() || response.dragged() {
            accent()
        } else {
            crate::theme::border()
        };
        ui.painter().line_segment(
            [handle.center_top(), handle.center_bottom()],
            egui::Stroke::new(1., color),
        );
        ui.painter().line_segment(
            [
                handle.center() - egui::vec2(0., 18.),
                handle.center() + egui::vec2(0., 18.),
            ],
            egui::Stroke::new(3., color),
        );
        let mut action = None;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            ui.allocate_ui_with_layout(
                egui::vec2(width, area.height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    // Clip directory widgets to the pane so overflowing buttons cannot
                    // cover the resize handle and steal its drag.
                    ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                    action = self.render_book_navigation(ui, book);
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(area.width() - width - GAP, area.height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| self.render_writing_paper(ui, format),
            );
        });
        if let Some(action) = action {
            self.apply_directory_action(action);
        }
    }

    fn apply_directory_action(&mut self, action: DirectoryAction) {
        match action {
            DirectoryAction::Drop(action) => {
                self.apply_library_drop(action);
            }
            DirectoryAction::Parent => self.open_parent_book(),
            DirectoryAction::Create => self.new_child_book(),
            DirectoryAction::Open(id) => {
                self.open_book_directory_page(&id);
            }
            DirectoryAction::Delete(id, title) => {
                self.request_library_deletion(&id, &title, true);
            }
        }
    }

    pub(super) fn open_book_directory_page(&mut self, id: &str) -> bool {
        let directory = self.library.book.clone();
        if !self.open_writing(id) {
            return false;
        }
        if let Some(book) = directory
            && self
                .writing
                .page
                .as_ref()
                .is_some_and(|page| self.book_is_within(&page.book_id, &book.id))
        {
            if self.writing.book_navigation.book_id != book.id {
                self.writing.book_navigation.book_id = book.id.clone();
                self.writing.book_navigation.revision = None;
            }
            self.library.book = Some(book);
        }
        true
    }

    pub(super) fn render_writing_zoom(&mut self, ui: &mut egui::Ui) {
        let current = ui.ctx().zoom_factor();
        let mut zoom = current.clamp(0.8, 1.5);
        ui.menu_button(format!("缩放 {}%", (zoom * 100.).round() as i32), |ui| {
            ui.label("界面与文字大小");
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(zoom > 0.8, egui::Button::new("缩小"))
                    .clicked()
                {
                    zoom = (zoom - 0.1).max(0.8);
                }
                if ui.button("重置 100%").clicked() {
                    zoom = 1.;
                }
                if ui
                    .add_enabled(zoom < 1.5, egui::Button::new("放大"))
                    .clicked()
                {
                    zoom = (zoom + 0.1).min(1.5);
                }
            });
            ui.add(
                egui::Slider::new(&mut zoom, 0.8..=1.5)
                    .show_value(false)
                    .text(format!("{}%", (self.theme.zoom * 100.).round() as i32)),
            );
            ui.label("也可使用 Ctrl + 加号 / 减号，Ctrl+0 重置。");
        });
        if (zoom - self.theme.zoom).abs() > 0.001 || (zoom - current).abs() > 0.001 {
            // Save only the zoom preference; do not commit unrelated theme edits.
            let mut saved = self.theme_saved.clone();
            saved.zoom = zoom;
            let result = serde_json::to_string(&saved)
                .map_err(DeskError::from)
                .and_then(|json| {
                    self.db
                        .as_mut()
                        .ok_or(DeskError::UnsupportedFormat)?
                        .set_setting("appearance.theme", &json)
                });
            match result {
                Ok(()) => {
                    self.theme.zoom = zoom;
                    self.theme_saved.zoom = zoom;
                    self.theme.apply(ui.ctx());
                    self.theme_applied = Some(self.theme.clone());
                }
                Err(error) => self.notice = format!("缩放未保存：{error}"),
            }
        }
    }

    fn render_book_navigation(
        &mut self,
        ui: &mut egui::Ui,
        book: &Book,
    ) -> Option<DirectoryAction> {
        if self.writing.book_navigation.book_id != book.id {
            self.writing.book_navigation = BookNavigation {
                book_id: book.id.clone(),
                ..Default::default()
            };
        }
        let revision = self.db.as_ref().map(|db| db.change_count());
        let navigation = &mut self.writing.book_navigation;
        if navigation.revision != revision {
            let loaded = self
                .db
                .as_ref()
                .ok_or(DeskError::UnsupportedFormat)
                .and_then(|db| {
                    let books = db.books()?;
                    let mut ids = vec![book.id.clone()];
                    let mut seen = HashSet::from([book.id.clone()]);
                    let mut index = 0;
                    while index < ids.len() {
                        let parent = ids[index].clone();
                        index += 1;
                        for child in &books {
                            if child.parent_id.as_deref() == Some(parent.as_str())
                                && seen.insert(child.id.clone())
                            {
                                ids.push(child.id.clone());
                            }
                        }
                    }
                    let mut pages = HashMap::new();
                    for id in ids {
                        pages.insert(id.clone(), db.book_pages(&id)?);
                    }
                    Ok((books, pages))
                });
            match loaded {
                Ok((books, pages)) => {
                    navigation.books = books;
                    navigation.pages = pages;
                    navigation.revision = revision;
                    navigation.error.clear();
                }
                Err(error) => navigation.error = format!("目录读取失败：{error}"),
            }
        }
        let draft = self.writing.page.as_ref();
        let mut parent = false;
        let mut create = false;
        let mut actions = DirectoryActions::default();
        egui::Frame::new()
            .fill(surface())
            .corner_radius(12)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    parent = ui.small_button("← 上一级").clicked();
                    create = ui.small_button("新建子图书").clicked();
                });
                ui.label(RichText::new("页面目录").strong().color(ink()));
                // Consume this before the body TextEdit: Ctrl+K otherwise deletes text there.
                let focus_search =
                    ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::K));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut navigation.query)
                        .id_salt(("book-page-search", &book.id))
                        .hint_text("搜索标题或正文")
                        .desired_width(f32::INFINITY),
                );
                self.search_field = Some(response.id);
                if focus_search {
                    self.writing.focus_body = false;
                    response.request_focus();
                }
                if !navigation.query.is_empty() && ui.small_button("清除搜索").clicked() {
                    navigation.query.clear();
                    response.request_focus();
                }
                let total = navigation.pages.values().map(Vec::len).sum::<usize>();
                let matched = navigation
                    .pages
                    .values()
                    .flatten()
                    .map(|page| current_page(page, draft))
                    .filter(|page| matches(page, &navigation.query))
                    .count();
                ui.label(
                    RichText::new(format!("{matched} / {total} 页"))
                        .small()
                        .color(muted()),
                );
                if !navigation.error.is_empty() {
                    ui.colored_label(Color32::from_rgb(184, 65, 71), &navigation.error);
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt(("book-page-directory", &book.id))
                    .auto_shrink([false, false])
                    .max_height(ui.available_height())
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        let BookNavigation {
                            books,
                            pages,
                            expanded,
                            query,
                            ..
                        } = navigation;
                        child_books(
                            ui,
                            &book.id,
                            (books, pages),
                            expanded,
                            query,
                            draft,
                            &mut actions,
                        );
                        for page in pages.get(&book.id).map(Vec::as_slice).unwrap_or(&[]) {
                            let page = current_page(page, draft);
                            if matches(page, query) {
                                page_row(ui, page, draft, query, &mut actions);
                            }
                        }
                        if matched == 0 {
                            ui.label("没有匹配的页面");
                            ui.label(
                                RichText::new("试试其他关键词，或清除搜索。")
                                    .small()
                                    .color(muted()),
                            );
                        }
                    });
            });
        if let Some(action) = actions.drop {
            return Some(DirectoryAction::Drop(action));
        }
        if parent {
            return Some(DirectoryAction::Parent);
        }
        if create {
            return Some(DirectoryAction::Create);
        }
        if let Some(id) = actions.select {
            return Some(DirectoryAction::Open(id));
        }
        if let Some((id, title)) = actions.remove {
            return Some(DirectoryAction::Delete(id, title));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::editor_tests::pointer;

    fn frame(
        app: &mut PersonalDeskApp,
        ctx: &egui::Context,
        size: egui::Vec2,
        time: f64,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| app.render_workspace(ui),
        );
        output.textures_delta.clear();
        output
    }

    fn positions(out: &egui::FullOutput, text: &str) -> Vec<egui::Pos2> {
        out.shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.text() == text => Some(t.pos + egui::vec2(4., 5.)),
                _ => None,
            })
            .collect()
    }

    fn fixture() -> (tempfile::TempDir, PersonalDeskApp, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
        let mut book = Book::new();
        book.title = "日记".into();
        let db = app.db.as_mut().unwrap();
        db.save_book(&book).unwrap();
        let first = db
            .create_writing("2026-07-07", Some(&book.id), None)
            .unwrap();
        let mut second = db
            .create_writing("2026-08-29", Some(&book.id), None)
            .unwrap();
        second.body = format!("{}今天进行 Rust 复盘。", "这是一段日记。".repeat(50));
        db.save_book_page(&mut second).unwrap();
        for index in 0..28 {
            db.create_writing(&format!("其他日记 {index}"), Some(&book.id), None)
                .unwrap();
        }
        app.section = Section::Books;
        app.open_writing(&first.id);
        (dir, app, first.id, second.id)
    }

    #[test]
    fn nested_books_expand_and_select_pages_without_replacing_the_directory() {
        for catalog in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
            let mut root = Book::new();
            root.title = "根图书".into();
            let mut child = Book::new();
            child.title = "二级图书".into();
            child.parent_id = Some(root.id.clone());
            let mut grandchild = Book::new();
            grandchild.title = "三级图书".into();
            grandchild.parent_id = Some(child.id.clone());
            let db = app.db.as_mut().unwrap();
            for book in [&root, &child, &grandchild] {
                db.save_book(book).unwrap();
            }
            let first = db.create_writing("父级文档", Some(&root.id), None).unwrap();
            let second = db
                .create_writing("子级文档", Some(&child.id), None)
                .unwrap();
            let third = db
                .create_writing("深层检索词", Some(&grandchild.id), None)
                .unwrap();
            app.section = Section::Books;
            app.library.book = Some(root.clone());
            if !catalog {
                app.open_writing(&first.id);
            }
            let ctx = egui::Context::default();
            configure_theme(&ctx);
            let size = egui::vec2(1280., 800.);
            frame(&mut app, &ctx, size, 0., vec![]);
            let out = frame(&mut app, &ctx, size, 0.1, vec![]);
            assert!(positions(&out, "子级文档").is_empty());
            let at = positions(&out, "▸ 图书 · 二级图书")[0];
            frame(&mut app, &ctx, size, 0.2, pointer(at, true));
            frame(&mut app, &ctx, size, 0.22, pointer(at, false));
            let out = frame(&mut app, &ctx, size, 0.3, vec![]);
            assert_eq!(app.library.book.as_ref().unwrap().id, root.id);
            assert_eq!(
                app.writing.page.as_ref().map(|page| &page.id),
                (!catalog).then_some(&first.id)
            );
            let at = positions(&out, "子级文档")[0];
            frame(&mut app, &ctx, size, 0.4, pointer(at, true));
            frame(&mut app, &ctx, size, 0.42, pointer(at, false));
            let out = frame(&mut app, &ctx, size, 0.5, vec![]);
            assert_eq!(app.writing.page.as_ref().unwrap().id, second.id);
            assert_eq!(app.library.book.as_ref().unwrap().id, root.id);
            assert!(!positions(&out, "▾ 图书 · 二级图书").is_empty());
            app.writing.page.as_mut().unwrap().body = "切换前未保存正文".into();
            app.mark_writing_changed(0.5);
            let at = positions(&out, "▸ 图书 · 三级图书")[0];
            frame(&mut app, &ctx, size, 0.6, pointer(at, true));
            frame(&mut app, &ctx, size, 0.62, pointer(at, false));
            let out = frame(&mut app, &ctx, size, 0.7, vec![]);
            assert_eq!(app.writing.page.as_ref().unwrap().id, second.id);
            assert!(app.writing.dirty);
            let at = positions(&out, "深层检索词")[0];
            frame(&mut app, &ctx, size, 0.8, pointer(at, true));
            frame(&mut app, &ctx, size, 0.82, pointer(at, false));
            frame(&mut app, &ctx, size, 0.9, vec![]);
            assert_eq!(app.writing.page.as_ref().unwrap().id, third.id);
            assert_eq!(app.library.book.as_ref().unwrap().id, root.id);
            assert_eq!(
                app.db
                    .as_ref()
                    .unwrap()
                    .writing_page(&second.id)
                    .unwrap()
                    .unwrap()
                    .body,
                "切换前未保存正文"
            );
            // Search reaches descendants even after their parent was collapsed.
            app.writing.book_navigation.expanded.clear();
            app.writing.book_navigation.query = "深层检索词".into();
            let out = frame(&mut app, &ctx, size, 1., vec![]);
            assert!(!positions(&out, "深层检索词").is_empty());
            assert!(positions(&out, "子级文档").is_empty());
            app.writing.book_navigation.query.clear();
            let out = frame(&mut app, &ctx, size, 1.1, vec![]);
            assert!(!positions(&out, "▸ 图书 · 二级图书").is_empty());
            assert_eq!(app.writing.page.as_ref().unwrap().id, third.id);
            assert!(app.new_writing(false));
            assert_eq!(app.writing.page.as_ref().unwrap().book_id, grandchild.id);
        }
    }

    #[test]
    fn nested_directory_conflict_blocks_selection_and_return_without_losing_the_tree() {
        let (dir, mut app, first, _) = fixture();
        let root = app.library.book.clone().unwrap();
        let mut child = Book::new();
        child.title = "冲突子图书".into();
        child.parent_id = Some(root.id.clone());
        let db = app.db.as_mut().unwrap();
        db.save_book(&child).unwrap();
        let target = db
            .create_writing("待选择子页", Some(&child.id), None)
            .unwrap();
        app.writing
            .book_navigation
            .expanded
            .insert(child.id.clone());
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        let size = egui::vec2(1280., 800.);
        frame(&mut app, &ctx, size, 0., vec![]);
        // The initial directory load starts a fresh view.
        app.writing
            .book_navigation
            .expanded
            .insert(child.id.clone());
        let out = frame(&mut app, &ctx, size, 0.1, vec![]);
        app.writing.page.as_mut().unwrap().body = "当前窗口草稿".into();
        app.mark_writing_changed(0.15);
        let mut other = Database::open(dir.path()).unwrap();
        let mut page = other.writing_page(&first).unwrap().unwrap();
        page.body = "其他窗口已保存".into();
        other.save_book_page(&mut page).unwrap();
        let at = positions(&out, "待选择子页")[0];
        frame(&mut app, &ctx, size, 0.2, pointer(at, true));
        frame(&mut app, &ctx, size, 0.22, pointer(at, false));
        let out = frame(&mut app, &ctx, size, 0.3, vec![]);
        assert_eq!(app.writing.page.as_ref().unwrap().id, first);
        assert_eq!(app.writing.page.as_ref().unwrap().body, "当前窗口草稿");
        assert!(app.writing.dirty && !app.writing.error.is_empty());
        assert!(app.writing.book_navigation.expanded.contains(&child.id));
        let at = positions(&out, "← 上一级")[0];
        frame(&mut app, &ctx, size, 0.4, pointer(at, true));
        frame(&mut app, &ctx, size, 0.42, pointer(at, false));
        frame(&mut app, &ctx, size, 0.5, vec![]);
        assert_eq!(app.library.book.as_ref().unwrap().id, root.id);
        assert_eq!(app.writing.page.as_ref().unwrap().id, first);
        assert_eq!(
            other.writing_page(&first).unwrap().unwrap().body,
            "其他窗口已保存"
        );
        assert!(
            other
                .writing_page(&target.id)
                .unwrap()
                .unwrap()
                .body
                .is_empty()
        );
    }

    #[test]
    fn returning_from_child_directory_renders_safely_and_saves_the_draft() {
        let (_dir, mut app, first, _) = fixture();
        let parent = app.library.book.clone().unwrap();
        let mut child = Book::new();
        child.title = "子图书".into();
        child.parent_id = Some(parent.id.clone());
        let db = app.db.as_mut().unwrap();
        db.save_book(&child).unwrap();
        let page = db
            .create_writing("子图书文档", Some(&child.id), None)
            .unwrap();
        assert!(app.enter_book(child));
        app.writing.page.as_mut().unwrap().body = "返回之前的草稿".into();
        app.mark_writing_changed(0.);
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        let size = egui::vec2(1000., 680.);
        frame(&mut app, &ctx, size, 0., vec![]);
        let out = frame(&mut app, &ctx, size, 0.1, vec![]);
        let at = positions(&out, "← 上一级")[0];
        frame(&mut app, &ctx, size, 0.2, pointer(at, true));
        frame(&mut app, &ctx, size, 0.22, pointer(at, false));
        frame(&mut app, &ctx, size, 0.3, vec![]);
        assert!(app.writing.page.is_none());
        assert_eq!(app.library.book.as_ref().unwrap().id, parent.id);
        assert_eq!(
            app.db
                .as_ref()
                .unwrap()
                .writing_page(&page.id)
                .unwrap()
                .unwrap()
                .body,
            "返回之前的草稿"
        );
        assert!(
            app.db
                .as_ref()
                .unwrap()
                .writing_page(&first)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn book_directory_is_vertical_and_searches_full_body_without_losing_the_draft() {
        for size in [egui::vec2(1000., 680.), egui::vec2(1280., 800.)] {
            let (_dir, mut app, first, second) = fixture();
            let ctx = egui::Context::default();
            configure_theme(&ctx);
            frame(&mut app, &ctx, size, 0., vec![]);
            let out = frame(&mut app, &ctx, size, 0.1, vec![]);
            let first_at = positions(&out, "2026-07-07");
            let second_at = positions(&out, "2026-08-29")[0];
            assert_eq!(first_at.len(), 2, "same page in directory and editor");
            assert!(first_at[0].x < first_at[1].x - 100.);
            assert!((first_at[0].x - second_at.x).abs() < 20. && second_at.y > first_at[0].y + 30.);
            assert!(first_at[1].x < size.x && first_at[1].y < size.y);
            assert!(!positions(&out, "30 / 30 页").is_empty());

            app.writing.page.as_mut().unwrap().body = "还没自动保存的日记".into();
            app.mark_writing_changed(0.2);
            // Use the real search control, including Ctrl+K, rather than setting a search result.
            frame(
                &mut app,
                &ctx,
                size,
                0.21,
                vec![egui::Event::Key {
                    key: egui::Key::K,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                }],
            );
            assert!(ctx.memory(|m| m.has_focus(app.search_field.unwrap())));
            frame(
                &mut app,
                &ctx,
                size,
                0.22,
                vec![egui::Event::Paste("rust".into())],
            );
            let out = frame(&mut app, &ctx, size, 0.23, vec![]);
            assert!(!positions(&out, "1 / 30 页").is_empty());
            assert_eq!(
                app.writing.page.as_ref().unwrap().id,
                first,
                "filtering must not switch the editor"
            );
            assert_eq!(
                app.writing.page.as_ref().unwrap().body,
                "还没自动保存的日记",
                "search must preserve draft text"
            );
            assert!(app.writing.dirty, "draft must stay dirty until switching");
            let at = positions(&out, "2026-08-29")[0];
            frame(&mut app, &ctx, size, 0.24, pointer(at, true));
            frame(&mut app, &ctx, size, 0.25, pointer(at, false));
            assert_eq!(app.writing.page.as_ref().unwrap().id, second);
            assert_eq!(
                app.db
                    .as_ref()
                    .unwrap()
                    .writing_page(&first)
                    .unwrap()
                    .unwrap()
                    .body,
                "还没自动保存的日记"
            );

            app.writing.book_navigation.query = "不存在的关键词".into();
            let out = frame(&mut app, &ctx, size, 0.26, vec![]);
            assert!(!positions(&out, "没有匹配的页面").is_empty());
            assert_eq!(app.writing.page.as_ref().unwrap().id, second);
            let at = positions(&out, "清除搜索")[0];
            frame(&mut app, &ctx, size, 0.27, pointer(at, true));
            frame(&mut app, &ctx, size, 0.28, pointer(at, false));
            let out = frame(&mut app, &ctx, size, 0.29, vec![]);
            assert!(!positions(&out, "30 / 30 页").is_empty());
        }
    }

    #[test]
    fn directory_switch_preserves_content_when_a_concurrent_save_blocks_navigation() {
        let (dir, mut app, first, _second) = fixture();
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        let size = egui::vec2(1000., 680.);
        frame(&mut app, &ctx, size, 0., vec![]);
        let out = frame(&mut app, &ctx, size, 0.1, vec![]);
        app.writing.page.as_mut().unwrap().body = "本窗口尚未保存的正文".into();
        app.mark_writing_changed(0.2);
        let mut other = Database::open(dir.path()).unwrap();
        let mut page = other.writing_page(&first).unwrap().unwrap();
        page.body = "其他窗口保存的正文".into();
        other.save_book_page(&mut page).unwrap();
        let at = positions(&out, "2026-08-29")[0];
        frame(&mut app, &ctx, size, 0.21, pointer(at, true));
        frame(&mut app, &ctx, size, 0.22, pointer(at, false));
        assert_eq!(app.writing.page.as_ref().unwrap().id, first);
        assert_eq!(
            app.writing.page.as_ref().unwrap().body,
            "本窗口尚未保存的正文"
        );
        assert!(app.writing.dirty && !app.writing.error.is_empty());
        assert_eq!(
            other.writing_page(&first).unwrap().unwrap().body,
            "其他窗口保存的正文"
        );
    }

    #[test]
    fn dragging_directory_edge_resizes_both_panes_and_remembers_width_after_reopening() {
        let (dir, mut app, first, _) = fixture();
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        let size = egui::vec2(1280., 800.);
        frame(&mut app, &ctx, size, 0., vec![]);
        let before = frame(&mut app, &ctx, size, 0.1, vec![]);
        let paper_before = positions(&before, "2026-07-07")[1];
        let handle = ctx
            .read_response(egui::Id::unique("book-directory-resize"))
            .unwrap()
            .rect
            .center();
        frame(&mut app, &ctx, size, 0.2, pointer(handle, true));
        let destination = handle + egui::vec2(140., 0.);
        frame(
            &mut app,
            &ctx,
            size,
            0.3,
            vec![egui::Event::PointerMoved(destination)],
        );
        frame(&mut app, &ctx, size, 0.4, pointer(destination, false));
        let after = frame(&mut app, &ctx, size, 0.5, vec![]);
        assert!(positions(&after, "2026-07-07")[1].x > paper_before.x + 110.);
        let saved = app.db.as_ref().unwrap().settings().unwrap()["writing.directory.width"]
            .parse::<f32>()
            .unwrap();
        assert!(saved > 380.);
        let small = frame(&mut app, &ctx, egui::vec2(1000., 680.), 0.6, vec![]);
        assert!(positions(&small, "2026-07-07")[1].x < 1000.);
        drop(app);
        let mut reopened = PersonalDeskApp::open_data_directory(dir.path().into());
        reopened.open_writing(&first);
        reopened.section = Section::Books;
        let fresh = egui::Context::default();
        configure_theme(&fresh);
        frame(&mut reopened, &fresh, size, 0., vec![]);
        assert!((reopened.writing.book_directory_width.unwrap() - saved).abs() < 1.);
        let handle = fresh
            .read_response(egui::Id::unique("book-directory-resize"))
            .unwrap()
            .rect
            .center();
        for (time, pressed) in [(0.2, true), (0.22, false), (0.3, true), (0.32, false)] {
            frame(&mut reopened, &fresh, size, time, pointer(handle, pressed));
        }
        assert!(
            reopened.writing.book_directory_width.unwrap() < 300.,
            "double-click restores the default directory width"
        );
        reopened.theme.zoom = 1.5;
        let compact = egui::vec2(1000. / 1.5, 680. / 1.5);
        frame(&mut reopened, &fresh, compact, 0.5, vec![]);
        frame(&mut reopened, &fresh, compact, 0.6, vec![]);
        let body = fresh
            .read_response(egui::Id::unique(("writing-body", &first)))
            .unwrap();
        assert!(
            body.rect.right() <= compact.x,
            "zoomed paper must reflow inside the remaining pane"
        );
    }

    #[test]
    fn narrow_directory_stays_resizable_when_its_header_needs_to_wrap() {
        let (_dir, mut app, _first, _) = fixture();
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        let size = egui::vec2(1280., 800.);
        let id = egui::Id::unique("book-directory-resize");
        frame(&mut app, &ctx, size, 0., vec![]);
        frame(&mut app, &ctx, size, 0.1, vec![]);
        let handle = ctx.read_response(id).unwrap().rect.center();
        let left = handle.x - app.writing.book_directory_width.unwrap() - 6.;
        frame(&mut app, &ctx, size, 0.2, pointer(handle, true));
        let narrow = egui::pos2(left + 156., handle.y);
        frame(
            &mut app,
            &ctx,
            size,
            0.3,
            vec![egui::Event::PointerMoved(narrow)],
        );
        frame(&mut app, &ctx, size, 0.4, pointer(narrow, false));
        frame(&mut app, &ctx, size, 0.5, vec![]);
        assert_eq!(app.writing.book_directory_width, Some(150.));

        let handle = ctx.read_response(id).unwrap().rect.center();
        frame(&mut app, &ctx, size, 0.6, pointer(handle, true));
        let wider = handle + egui::vec2(120., 0.);
        frame(
            &mut app,
            &ctx,
            size,
            0.7,
            vec![egui::Event::PointerMoved(wider)],
        );
        frame(&mut app, &ctx, size, 0.8, pointer(wider, false));
        frame(&mut app, &ctx, size, 0.9, vec![]);
        assert!(
            app.writing.book_directory_width.unwrap() > 240.,
            "the resize handle must stay grabbable after the header wraps"
        );
    }

    #[test]
    fn writing_zoom_controls_save_only_zoom_and_preserve_text_and_unsaved_theme_colors() {
        let (dir, mut app, first, _) = fixture();
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        let size = egui::vec2(1280., 800.);
        frame(&mut app, &ctx, size, 0., vec![]);
        let out = frame(&mut app, &ctx, size, 0.1, vec![]);
        app.writing.page.as_mut().unwrap().body = "缩放时不改变正文".into();
        app.mark_writing_changed(0.15);
        app.theme.accent = [9, 99, 9];
        let at = positions(&out, "缩放 100%")[0];
        frame(&mut app, &ctx, size, 0.2, pointer(at, true));
        frame(&mut app, &ctx, size, 0.22, pointer(at, false));
        let menu = frame(&mut app, &ctx, size, 0.23, vec![]);
        let at = positions(&menu, "放大")[0];
        frame(&mut app, &ctx, size, 0.24, pointer(at, true));
        frame(&mut app, &ctx, size, 0.26, pointer(at, false));
        assert!((app.theme.zoom - 1.1).abs() < 0.001);
        assert_eq!(app.writing.page.as_ref().unwrap().id, first);
        assert_eq!(app.writing.page.as_ref().unwrap().body, "缩放时不改变正文");
        let restored = PersonalDeskApp::open_data_directory(dir.path().into());
        assert!((restored.theme.zoom - 1.1).abs() < 0.001);
        assert_ne!(restored.theme.accent, [9, 99, 9]);
        frame(
            &mut app,
            &ctx,
            size,
            0.3,
            vec![egui::Event::Key {
                key: egui::Key::Num0,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
        );
        frame(&mut app, &ctx, size, 0.4, vec![]);
        let restored = PersonalDeskApp::open_data_directory(dir.path().into());
        assert!(
            (restored.theme.zoom - 1.).abs() < 0.001,
            "keyboard zoom reset is also remembered"
        );
    }
}
