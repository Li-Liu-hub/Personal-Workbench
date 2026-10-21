use super::*;
impl PersonalDeskApp {
    pub(super) fn save_inline_inventory(&mut self, id: &str) -> bool {
        let Some(mut draft) = self.inline_inventory.get(id).cloned() else {
            return true;
        };
        if let Some(value) = self.inventory_quantities.get(id) {
            match value.trim().parse::<i64>() {
                Ok(number) if number >= 0 => draft.quantity = number,
                _ => {
                    self.notice = "数量需要填写大于等于 0 的整数".into();
                    return false;
                }
            }
        }
        if let Some(value) = self.inventory_prices.get(id) {
            match parse_price(value) {
                Ok(price) => draft.purchase_price_cents = price,
                Err(error) => {
                    self.notice = error;
                    return false;
                }
            }
        }
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
                self.inventory_prices.remove(id);
                self.inventory_quantities.remove(id);
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
        use crate::ui_components::table;
        let options = self.inventory_tags.clone();
        const C: &[f32] = &[135., 80., 105., 50., 90., 108., 90., 108., 95., 140.];
        let fitted = table::fit(C, ui.available_width());
        let columns = fitted.as_slice();
        ui.spacing_mut().scroll.floating = false;
        egui::ScrollArea::horizontal()
            .id_salt("inventory-columns")
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                table::header(
                    ui,
                    columns,
                    &[
                        "名称",
                        "品牌",
                        "型号",
                        "数量",
                        "存放位置",
                        "购买日期",
                        "购买价格",
                        "丢弃日期",
                        "分类",
                        "操作",
                    ],
                );
                for item in items {
                    let mut draft = self.inline_inventory.get(&item.id).unwrap_or(item).clone();
                    let mut changed = false;
                    let (response, cells) = table::row(ui, ("inventory-row", &item.id), columns);
                    for (index, value) in [
                        (0, &mut draft.name),
                        (1, &mut draft.brand),
                        (2, &mut draft.model),
                        (4, &mut draft.location),
                        (5, &mut draft.purchase_date),
                        (7, &mut draft.disposal_date),
                    ] {
                        changed |= table::text(ui, cells[index], (&item.id, index), value);
                        if matches!(index, 5 | 7) {
                            table::date_hint(ui, cells[index], value);
                        }
                    }
                    let mut quantity = self
                        .inventory_quantities
                        .get(&item.id)
                        .cloned()
                        .unwrap_or_else(|| draft.quantity.to_string());
                    if table::text(ui, cells[3], (&item.id, "quantity"), &mut quantity) {
                        self.inventory_quantities.insert(item.id.clone(), quantity);
                        changed = true;
                    }
                    let mut price =
                        self.inventory_prices
                            .get(&item.id)
                            .cloned()
                            .unwrap_or_else(|| {
                                draft
                                    .purchase_price_cents
                                    .map(|c| format!("{}.{:02}", c / 100, c % 100))
                                    .unwrap_or_default()
                            });
                    if table::text(ui, cells[6], (&item.id, "price"), &mut price) {
                        self.inventory_prices.insert(item.id.clone(), price);
                        changed = true;
                    }
                    table::cell_ui(ui, cells[8], |ui| {
                        changed |= crate::ui_components::classification_dropdown(
                            ui,
                            (&item.id, "row-tags"),
                            &mut draft.tags,
                            &options,
                            cells[8].width(),
                        );
                    });
                    let mut save = false;
                    let mut cancel = false;
                    let mut detail = false;
                    table::cell_ui(ui, cells[9], |ui| {
                        if changed || self.inline_inventory.contains_key(&item.id) {
                            save = ui.small_button("保存").clicked();
                            cancel = ui.small_button("取消").clicked();
                        }
                        if !changed && !self.inline_inventory.contains_key(&item.id) {
                            detail = ui.small_button("详情").clicked();
                        }
                    });
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
                        self.inventory_prices.remove(&item.id);
                        self.inventory_quantities.remove(&item.id);
                    } else if save {
                        self.save_inline_inventory(&item.id);
                    }
                    if detail {
                        // Commit pending row fields before opening another editor for this record.
                        if !self.inline_inventory.contains_key(&item.id)
                            || self.save_inline_inventory(&item.id)
                        {
                            self.inventory_draft = self
                                .inventory_items
                                .iter()
                                .find(|i| i.id == item.id)
                                .cloned()
                                .unwrap_or(draft);
                            self.selected_inventory = Some(item.id.clone());
                            self.inventory_dialog_open = true;
                        }
                    }
                }
                if table::add_row(ui, columns, "add-inventory-row", "添加物品").clicked() {
                    self.add_current_item();
                }
            });
    }
}

fn parse_price(value: &str) -> Result<Option<i64>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() > 2
        || parts[0].is_empty()
        || !parts[0].bytes().all(|b| b.is_ascii_digit())
        || parts
            .get(1)
            .is_some_and(|p| p.len() > 2 || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("购买价格请填写非负金额，最多两位小数".into());
    }
    let whole = parts[0].parse::<i64>().ok();
    let fraction = parts
        .get(1)
        .map(|s| format!("{s:0<2}").parse::<i64>())
        .transpose()
        .map_err(|_| "金额无效")?
        .unwrap_or(0);
    whole
        .and_then(|n| n.checked_mul(100))
        .and_then(|n| n.checked_add(fraction))
        .map(Some)
        .ok_or_else(|| "购买价格过大".into())
}
