use super::*;
impl PersonalDeskApp {
    pub(super) fn save_inline_inventory(&mut self, id: &str) -> bool {
        let Some(mut draft) = self.inline_inventory.get(id).cloned() else {
            return true;
        };
        draft.category = draft.tags.join(" · ");
        if draft.name.trim().is_empty() {
            self.notice = "请填写物品名称".into();
            return false;
        }
        match self
            .db
            .as_mut()
            .ok_or(DeskError::UnsupportedFormat)
            .and_then(|db| db.save_inventory_item(&draft))
        {
            Ok(()) => {
                self.inline_inventory.remove(id);
                self.load_inventory();
                self.load_tags();
                self.notice = "已保存".into();
                true
            }
            Err(error) => {
                self.notice = error.to_string();
                false
            }
        }
    }
    pub(super) fn render_inventory_rows(&mut self, ui: &mut egui::Ui, items: &[InventoryItem]) {
        let options = self.inventory_tags.clone();
        egui::ScrollArea::horizontal()
            .id_salt("inventory-columns")
            .show(ui, |ui| {
                let width = ui.available_width().max(1050.0);
                let columns = [0.16, 0.06, 0.10, 0.12, 0.10, 0.12, 0.14, 0.20];
                ui.horizontal(|ui| {
                    for (label, part) in [
                        "名称",
                        "数量",
                        "存放位置",
                        "购买日期",
                        "购买价格",
                        "丢弃日期",
                        "分类",
                        "操作",
                    ]
                    .into_iter()
                    .zip(columns)
                    {
                        ui.add_sized(
                            [width * part - 10.0, 25.0],
                            egui::Label::new(RichText::new(label).small().color(muted())),
                        );
                    }
                });
                for item in items {
                    let mut draft = self.inline_inventory.get(&item.id).unwrap_or(item).clone();
                    let mut changed = false;
                    let mut save = false;
                    let mut cancel = false;
                    let mut detail = false;
                    let response = egui::Frame::new()
                        .fill(surface())
                        .corner_radius(10)
                        .inner_margin(8)
                        .show(ui, |ui| {
                            ui.set_width(width);
                            ui.horizontal(|ui| {
                                changed |= ui
                                    .add_sized(
                                        [width * columns[0] - 10.0, 34.0],
                                        egui::TextEdit::singleline(&mut draft.name)
                                            .id_salt((&item.id, "name")),
                                    )
                                    .changed();
                                changed |= ui
                                    .add_sized(
                                        [width * columns[1] - 10.0, 34.0],
                                        egui::DragValue::new(&mut draft.quantity)
                                            .range(0..=i64::MAX),
                                    )
                                    .changed();
                                changed |= ui
                                    .add_sized(
                                        [width * columns[2] - 10.0, 34.0],
                                        egui::TextEdit::singleline(&mut draft.location)
                                            .id_salt((&item.id, "location")),
                                    )
                                    .changed();
                                changed |= ui
                                    .add_sized(
                                        [width * columns[3] - 10.0, 34.0],
                                        egui::TextEdit::singleline(&mut draft.purchase_date)
                                            .id_salt((&item.id, "purchase"))
                                            .hint_text("YYYY-MM-DD"),
                                    )
                                    .changed();
                                ui.allocate_ui(egui::vec2(width * columns[4] - 10.0, 34.0), |ui| {
                                    let mut has_price = draft.purchase_price_cents.is_some();
                                    if ui
                                        .checkbox(&mut has_price, "")
                                        .on_hover_text("填写购买价格")
                                        .changed()
                                    {
                                        draft.purchase_price_cents = has_price.then_some(0);
                                        changed = true;
                                    }
                                    if let Some(cents) = draft.purchase_price_cents.as_mut() {
                                        let mut value = *cents as f64 / 100.0;
                                        if ui
                                            .add(
                                                egui::DragValue::new(&mut value)
                                                    .fixed_decimals(2)
                                                    .range(0.0..=1e12)
                                                    .speed(1.0),
                                            )
                                            .changed()
                                        {
                                            *cents = (value * 100.0).round() as i64;
                                            changed = true;
                                        }
                                    }
                                });
                                changed |= ui
                                    .add_sized(
                                        [width * columns[5] - 10.0, 34.0],
                                        egui::TextEdit::singleline(&mut draft.disposal_date)
                                            .id_salt((&item.id, "disposal"))
                                            .hint_text("YYYY-MM-DD"),
                                    )
                                    .changed();
                                ui.allocate_ui(egui::vec2(width * columns[6] - 10.0, 34.0), |ui| {
                                    changed |= crate::ui_components::classification_dropdown(
                                        ui,
                                        (&item.id, "inventory-row-tags"),
                                        &mut draft.tags,
                                        &options,
                                        width * columns[6] - 18.0,
                                    );
                                });
                                ui.horizontal(|ui| {
                                    if changed || self.inline_inventory.contains_key(&item.id) {
                                        save = ui.small_button("保存").clicked();
                                        cancel = ui.small_button("取消").clicked();
                                    }
                                    detail = ui.small_button("详情").clicked();
                                });
                            });
                        })
                        .response;
                    self.favorite_control(ui, &response, "inventory", &item.id);
                    response.context_menu(|ui| {
                        self.record_tag_menu(ui, TagScope::Inventory, &item.id);
                        if ui.button("删除物品记录").clicked() {
                            self.delete_inventory_record(&item.id);
                            ui.close();
                        }
                    });
                    if changed {
                        self.inline_inventory.insert(item.id.clone(), draft.clone());
                    }
                    if cancel {
                        self.inline_inventory.remove(&item.id);
                    } else if save {
                        self.save_inline_inventory(&item.id);
                    }
                    if detail {
                        self.selected_inventory = Some(item.id.clone());
                        self.inventory_draft = draft;
                        self.inventory_dialog_open = true;
                    }
                }
                if ui
                    .add_sized([width, 45.0], egui::Button::new("+ 点击添加物品"))
                    .clicked()
                {
                    self.add_current_item();
                }
            });
    }
}
