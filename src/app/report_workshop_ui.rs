use super::*;
use crate::report_workshop::{
    self, CellValue, Field, FieldRule, ImportMode, Kind, RowRule, Rule, SheetImport, SheetPreview,
    Source, Template, TemplateCell,
};
use std::sync::mpsc::{self, Receiver, TryRecvError};

pub(super) const TOOL_ID: &str = "builtin:report-workshop";

struct DraftSheet {
    enabled: bool,
    preview: SheetPreview,
    plan: SheetImport,
    header_row: usize,
}

enum JobResult {
    Inspection(PathBuf, String, Vec<SheetPreview>),
    Import(Vec<usize>),
    Refreshed(usize),
    Generated(
        i64,
        String,
        Vec<(String, Vec<report_workshop::Decimal>)>,
        Vec<report_workshop::ResolvedCell>,
    ),
    Workbook(PathBuf, usize),
}

pub(super) struct ReportState {
    pub open: bool,
    page: usize,
    path: Option<PathBuf>,
    source_hash: String,
    sheets: Vec<DraftSheet>,
    active: usize,
    mode: ImportMode,
    confirm_replace: bool,
    tables: Vec<SheetImport>,
    templates: Vec<(String, i64, Template)>,
    workbook_sheets: Vec<(String, i64, Template)>,
    runs: Vec<(String, i64, i64, String, String, String)>,
    template: Template,
    template_id: String,
    saved_template: Option<(String, i64, String)>,
    pending_load: Option<(String, i64, Template)>,
    result: Vec<(String, Vec<report_workshop::Decimal>)>,
    result_cells: Vec<report_workshop::ResolvedCell>,
    result_signature: String,
    result_revision: i64,
    data_table: String,
    data_loaded_table: String,
    data_offset: usize,
    data_rows: Vec<(i64, Vec<Option<String>>)>,
    data_revision: i64,
    row_id: String,
    edit_field: String,
    edit_value: String,
    new_field: String,
    rename_to: String,
    new_kind: usize,
    refresh_rule: RowRule,
    confirm_refresh: bool,
    refresh_constant: bool,
    refresh_text: String,
    error: String,
    notice: String,
    job: Option<Receiver<std::result::Result<JobResult, String>>>,
}

impl Default for ReportState {
    fn default() -> Self {
        Self {
            open: false,
            page: 0,
            path: None,
            source_hash: String::new(),
            sheets: Vec::new(),
            active: 0,
            mode: ImportMode::Create,
            confirm_replace: false,
            tables: Vec::new(),
            templates: Vec::new(),
            workbook_sheets: Vec::new(),
            runs: Vec::new(),
            template: Template {
                title: "新报表".into(),
                group_table: String::new(),
                group_field: String::new(),
                group_filters: Vec::new(),
                sort_column: None,
                descending: false,
                limit: None,
                dynamic_row: 2,
                group_column: 0,
                columns: Vec::new(),
                cells: Vec::new(),
            },
            template_id: String::new(),
            saved_template: None,
            pending_load: None,
            result: Vec::new(),
            result_cells: Vec::new(),
            result_signature: String::new(),
            result_revision: 0,
            data_table: String::new(),
            data_loaded_table: String::new(),
            data_offset: 0,
            data_rows: Vec::new(),
            data_revision: 0,
            row_id: String::new(),
            edit_field: String::new(),
            edit_value: String::new(),
            new_field: String::new(),
            rename_to: String::new(),
            new_kind: 0,
            refresh_rule: RowRule::Field(String::new()),
            confirm_refresh: false,
            refresh_constant: false,
            refresh_text: String::new(),
            error: String::new(),
            notice: String::new(),
            job: None,
        }
    }
}

impl ReportState {
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
    fn inspect(&mut self, path: PathBuf) {
        let (sender, receiver) = mpsc::channel();
        self.job = Some(receiver);
        self.notice = "正在读取工作簿…".into();
        std::thread::spawn(move || {
            let result = (|| -> crate::error::Result<_> {
                let hash = report_workshop::file_hash(&path)?;
                let sheets = report_workshop::inspect_workbook(&path)?;
                if report_workshop::file_hash(&path)? != hash {
                    return Err(crate::error::DeskError::ConcurrentModification);
                }
                Ok(JobResult::Inspection(path, hash, sheets))
            })()
            .map_err(|e| e.to_string());
            let _ = sender.send(result);
        });
    }
    fn import(
        &mut self,
        db_path: PathBuf,
        path: PathBuf,
        plans: Vec<SheetImport>,
        mode: ImportMode,
    ) {
        let (sender, receiver) = mpsc::channel();
        self.job = Some(receiver);
        self.notice = "正在导入数据…".into();
        let hash = self.source_hash.clone();
        std::thread::spawn(move || {
            let result = (|| -> crate::error::Result<_> {
                let mut db = rusqlite::Connection::open_with_flags(
                    db_path,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
                )?;
                db.busy_timeout(std::time::Duration::from_secs(3))?;
                report_workshop::import_tables_checked(&mut db, &path, &plans, mode, Some(&hash))
            })()
            .map(JobResult::Import)
            .map_err(|e| e.to_string());
            let _ = sender.send(result);
        });
    }
    fn refresh(
        &mut self,
        db_path: PathBuf,
        table: String,
        field: String,
        rule: RowRule,
        revision: i64,
        constant: Option<String>,
    ) {
        let (sender, receiver) = mpsc::channel();
        self.job = Some(receiver);
        self.notice = "正在按规则刷数…".into();
        std::thread::spawn(move || {
            let result = (|| -> crate::error::Result<_> {
                let mut db = rusqlite::Connection::open_with_flags(
                    db_path,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
                )?;
                db.busy_timeout(std::time::Duration::from_secs(3))?;
                if let Some(text) = constant {
                    report_workshop::fill_field(&mut db, &table, &field, &text, revision)
                } else {
                    report_workshop::refresh_field(&mut db, &table, &field, &rule, revision)
                }
            })()
            .map(JobResult::Refreshed)
            .map_err(|e| e.to_string());
            let _ = sender.send(result);
        });
    }
    fn generate(&mut self, db_path: PathBuf, template: Template) {
        let (sender, receiver) = mpsc::channel();
        self.job = Some(receiver);
        self.notice = "正在计算报表…".into();
        std::thread::spawn(move || {
            let result = (|| -> crate::error::Result<_> {
                let db = rusqlite::Connection::open_with_flags(
                    db_path,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )?;
                db.busy_timeout(std::time::Duration::from_secs(3))?;
                let tx = db.unchecked_transaction()?;
                let revision = tx.query_row(
                    "SELECT value FROM report_workshop_meta WHERE key='data_revision'",
                    [],
                    |r| r.get(0),
                )?;
                let rows = report_workshop::generate_rows(&tx, &template)?;
                let cells = report_workshop::generate_cells(&tx, &template)?;
                let signature = serde_json::to_string(&template)?;
                tx.commit()?;
                Ok(JobResult::Generated(revision, signature, rows, cells))
            })()
            .map_err(|e| e.to_string());
            let _ = sender.send(result);
        });
    }
    fn export_workbook(
        &mut self,
        db_path: PathBuf,
        path: PathBuf,
        templates: Vec<(String, i64, Template)>,
    ) {
        let (sender, receiver) = mpsc::channel();
        self.job = Some(receiver);
        self.notice = "正在生成多工作表报表…".into();
        std::thread::spawn(move || {
            let result = (|| -> crate::error::Result<_> {
                let mut db = rusqlite::Connection::open_with_flags(
                    db_path,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
                )?;
                db.busy_timeout(std::time::Duration::from_secs(3))?;
                let tx = db.unchecked_transaction()?;
                let revision: i64 = tx.query_row(
                    "SELECT value FROM report_workshop_meta WHERE key='data_revision'",
                    [],
                    |r| r.get(0),
                )?;
                let mut generated = Vec::new();
                for (_, _, template) in &templates {
                    generated.push((
                        report_workshop::generate_rows(&tx, template)?,
                        report_workshop::generate_cells(&tx, template)?,
                    ));
                }
                tx.commit()?;
                let reports = templates
                    .iter()
                    .zip(&generated)
                    .map(|((_, _, template), (rows, cells))| {
                        (template, rows.as_slice(), cells.as_slice())
                    })
                    .collect::<Vec<_>>();
                report_workshop::export_workbook(&path, &reports)?;
                report_workshop::record_workbook_export(&mut db, &templates, revision, &path)
                    .map_err(|e| {
                        crate::error::DeskError::InvalidInput(format!(
                            "文件已写入 {}，但生成记录保存失败：{e}",
                            path.display()
                        ))
                    })?;
                Ok(JobResult::Workbook(path, templates.len()))
            })()
            .map_err(|e| e.to_string());
            let _ = sender.send(result);
        });
    }
    pub fn poll(&mut self, db: Option<&Database>) {
        let result = match self.job.as_ref().map(|r| r.try_recv()) {
            Some(Ok(result)) => result,
            Some(Err(TryRecvError::Disconnected)) => Err("后台任务意外中断".into()),
            _ => return,
        };
        self.job = None;
        match result {
            Ok(JobResult::Inspection(path, hash, sheets)) => {
                self.path = Some(path);
                self.source_hash = hash;
                self.sheets = sheets.into_iter().map(initial_plan).collect();
                self.active = 0;
                self.notice = "工作簿已读取".into();
                self.error.clear();
            }
            Ok(JobResult::Import(counts)) => {
                self.result.clear();
                self.data_rows.clear();
                self.data_loaded_table.clear();
                self.result_signature.clear();
                self.notice = format!("导入成功，各表行数：{counts:?}");
                self.error.clear();
                if let Some(db) = db {
                    match db.report_tables() {
                        Ok(t) => self.tables = t,
                        Err(e) => self.error = e.to_string(),
                    }
                }
            }
            Ok(JobResult::Generated(revision, signature, rows, cells)) => {
                self.result = rows;
                self.result_cells = cells;
                self.result_revision = revision;
                self.result_signature = signature;
                self.notice = format!("生成 {} 行", self.result.len());
                self.error.clear();
            }
            Ok(JobResult::Refreshed(count)) => {
                self.data_rows.clear();
                self.data_loaded_table.clear();
                self.result.clear();
                self.notice = format!("已按规则更新 {count} 行，请重新读取数据");
                self.error.clear();
            }
            Ok(JobResult::Workbook(path, count)) => {
                self.notice = format!("已导出 {} 个工作表：{}", count, path.display());
                self.error.clear();
                if let Some(db) = db {
                    match db.report_runs() {
                        Ok(runs) => self.runs = runs,
                        Err(e) => self.error = e.to_string(),
                    }
                }
            }
            Err(error) => {
                self.notice.clear();
                self.error = error;
            }
        }
    }
}

fn initial_plan(preview: SheetPreview) -> DraftSheet {
    let fields = (0..preview.width)
        .map(|i| Field {
            name: preview
                .rows
                .first()
                .and_then(|r| r.get(i))
                .filter(|v| !v.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| format!("字段{}", i + 1)),
            kind: Kind::Text,
            source: Source::Column(i),
            required: false,
        })
        .collect();
    let plan = SheetImport {
        sheet: preview.name.clone(),
        table: preview.name.clone(),
        first_data_row: 2,
        end_marker: String::new(),
        skip_empty_column: None,
        fields,
    };
    DraftSheet {
        enabled: true,
        preview,
        plan,
        header_row: 1,
    }
}

fn choose(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut String,
    choices: impl IntoIterator<Item = String>,
) {
    egui::ComboBox::from_id_salt(id)
        .selected_text(if value.is_empty() {
            "请选择"
        } else {
            value.as_str()
        })
        .show_ui(ui, |ui| {
            for choice in choices {
                ui.selectable_value(value, choice.clone(), choice);
            }
        });
}

fn edit_rule(ui: &mut egui::Ui, rule: &mut Rule, tables: &[SheetImport], id: String, depth: usize) {
    let old = match rule {
        Rule::Number(_) => 0,
        Rule::Sum { .. } => 1,
        Rule::Add(..) => 2,
        Rule::Subtract(..) => 3,
        Rule::Multiply(..) => 4,
        Rule::Divide(..) => 5,
        Rule::LinkedSum { .. } => 6,
    };
    let mut kind = old;
    egui::ComboBox::from_id_salt((&id, "kind"))
        .selected_text(["常数", "按当前行汇总", "加", "减", "乘", "除", "关联后汇总"][kind])
        .show_ui(ui, |ui| {
            for (i, label) in ["常数", "按当前行汇总", "加", "减", "乘", "除", "关联后汇总"]
                .iter()
                .enumerate()
            {
                if depth < 8 || i < 2 || i == 6 {
                    ui.selectable_value(&mut kind, i, *label);
                }
            }
        });
    if kind != old {
        let n = || Box::new(Rule::Number("0".into()));
        *rule = match kind {
            0 => Rule::Number("0".into()),
            1 => Rule::Sum {
                table: String::new(),
                field: String::new(),
                group_field: String::new(),
                filters: Vec::new(),
                all_rows: false,
            },
            2 => Rule::Add(n(), n()),
            3 => Rule::Subtract(n(), n()),
            4 => Rule::Multiply(n(), n()),
            5 => Rule::Divide(n(), n()),
            _ => Rule::LinkedSum {
                left_table: String::new(),
                right_table: String::new(),
                amount_field: String::new(),
                group_field: String::new(),
                keys: vec![(String::new(), String::new())],
                right_filters: Vec::new(),
            },
        };
    }
    match rule {
        Rule::Number(value) => {
            ui.label("数字");
            ui.add(egui::TextEdit::singleline(value).desired_width(150.));
        }
        Rule::Sum {
            table,
            field,
            group_field,
            filters,
            all_rows,
        } => {
            ui.horizontal_wrapped(|ui| {
                ui.label("表");
                choose(
                    ui,
                    (&id, "table"),
                    table,
                    tables.iter().map(|p| p.table.clone()),
                );
                let fields = tables
                    .iter()
                    .find(|p| p.table == *table)
                    .map(|p| p.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>())
                    .unwrap_or_default();
                ui.label("金额字段");
                choose(ui, (&id, "field"), field, fields.clone());
                ui.checkbox(all_rows, "汇总全部匹配行");
                if !*all_rows {
                    ui.label("匹配当前行的字段");
                    choose(ui, (&id, "group"), group_field, fields);
                }
            });
            let fields = tables
                .iter()
                .find(|p| p.table == *table)
                .map(|p| p.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>())
                .unwrap_or_default();
            let mut remove = None;
            for (i, (field, value)) in filters.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label("筛选字段");
                    choose(ui, (&id, "filter", i), field, fields.clone());
                    ui.label("等于");
                    ui.add(egui::TextEdit::singleline(value).desired_width(150.));
                    if ui.small_button("移除").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                filters.remove(i);
            }
            if ui.button("增加筛选条件").clicked() {
                filters.push((String::new(), String::new()));
            }
        }
        Rule::LinkedSum {
            left_table,
            right_table,
            amount_field,
            group_field,
            keys,
            right_filters,
        } => {
            ui.small("对左表匹配右表的记录汇总；右表重复行不会放大金额。");
            ui.horizontal_wrapped(|ui| {
                ui.label("左表");
                choose(
                    ui,
                    (&id, "left-table"),
                    left_table,
                    tables.iter().map(|p| p.table.clone()),
                );
                ui.label("右表");
                choose(
                    ui,
                    (&id, "right-table"),
                    right_table,
                    tables.iter().map(|p| p.table.clone()),
                );
            });
            let fields = |name: &str| {
                tables
                    .iter()
                    .find(|p| p.table == name)
                    .map(|p| p.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>())
                    .unwrap_or_default()
            };
            let left = fields(left_table);
            let right = fields(right_table);
            ui.horizontal_wrapped(|ui| {
                ui.label("左表金额字段");
                choose(ui, (&id, "amount"), amount_field, left.clone());
                ui.label("左表分组字段");
                choose(ui, (&id, "left-group"), group_field, left.clone());
            });
            let mut remove = None;
            for (i, (a, b)) in keys.iter_mut().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.label("关联字段");
                    choose(ui, (&id, "key-left", i), a, left.clone());
                    ui.label("等于");
                    choose(ui, (&id, "key-right", i), b, right.clone());
                    if ui.small_button("移除").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                keys.remove(i);
            }
            if ui.button("增加关联字段").clicked() {
                keys.push((String::new(), String::new()));
            }
            let mut remove = None;
            for (i, (name, value)) in right_filters.iter_mut().enumerate() {
                ui.horizontal_wrapped(|ui| {
                    ui.label("右表筛选");
                    choose(ui, (&id, "rf", i), name, right.clone());
                    ui.label("等于");
                    ui.add(egui::TextEdit::singleline(value).desired_width(150.));
                    if ui.small_button("移除").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                right_filters.remove(i);
            }
            if ui.button("增加右表筛选").clicked() {
                right_filters.push((String::new(), String::new()));
            }
        }
        Rule::Add(a, b) | Rule::Subtract(a, b) | Rule::Multiply(a, b) | Rule::Divide(a, b) => {
            ui.indent((&id, "left"), |ui| {
                ui.label("左侧");
                edit_rule(ui, a, tables, format!("{id}l"), depth + 1);
            });
            ui.indent((&id, "right"), |ui| {
                ui.label("右侧");
                edit_rule(ui, b, tables, format!("{id}r"), depth + 1);
            });
        }
    }
}

fn edit_row_rule(
    ui: &mut egui::Ui,
    rule: &mut RowRule,
    fields: &[String],
    id: String,
    depth: usize,
) {
    let old = match rule {
        RowRule::Field(_) => 0,
        RowRule::Number(_) => 1,
        RowRule::Add(..) => 2,
        RowRule::Subtract(..) => 3,
        RowRule::Multiply(..) => 4,
        RowRule::Divide(..) => 5,
    };
    let mut kind = old;
    egui::ComboBox::from_id_salt((&id, "row-rule"))
        .selected_text(["字段", "数字", "加", "减", "乘", "除"][kind])
        .show_ui(ui, |ui| {
            for (i, label) in ["字段", "数字", "加", "减", "乘", "除"].iter().enumerate() {
                if depth < 8 || i < 2 {
                    ui.selectable_value(&mut kind, i, *label);
                }
            }
        });
    if kind != old {
        let n = || Box::new(RowRule::Number("0".into()));
        *rule = match kind {
            0 => RowRule::Field(String::new()),
            1 => RowRule::Number("0".into()),
            2 => RowRule::Add(n(), n()),
            3 => RowRule::Subtract(n(), n()),
            4 => RowRule::Multiply(n(), n()),
            _ => RowRule::Divide(n(), n()),
        };
    }
    match rule {
        RowRule::Field(field) => choose(ui, (&id, "field"), field, fields.iter().cloned()),
        RowRule::Number(n) => {
            ui.add(egui::TextEdit::singleline(n).desired_width(120.));
        }
        RowRule::Add(a, b)
        | RowRule::Subtract(a, b)
        | RowRule::Multiply(a, b)
        | RowRule::Divide(a, b) => {
            ui.indent((&id, "a"), |ui| {
                ui.label("左侧");
                edit_row_rule(ui, a, fields, format!("{id}a"), depth + 1);
            });
            ui.indent((&id, "b"), |ui| {
                ui.label("右侧");
                edit_row_rule(ui, b, fields, format!("{id}b"), depth + 1);
            });
        }
    }
}

impl PersonalDeskApp {
    pub(super) fn render_report_workshop_entry(
        &mut self,
        ui: &mut egui::Ui,
        layout: crate::ui_components::EntryLayout,
    ) {
        if self.record_matches_filter(TagScope::Tools, TOOL_ID)
            && matches_query(
                &self.search_query.to_lowercase(),
                &["数据建模与报表工坊", "excel 数据 表 模板 报表 sqlite"],
            )
        {
            let response = entry(
                ui,
                TOOL_ID,
                "数据建模与报表工坊",
                "导入工作表、配置计算规则",
                TileKind::Document,
                layout,
            );
            self.favorite_control(ui, &response, "tools", TOOL_ID);
            if response.clicked() {
                self.report_workshop.open = true;
                if let Some(db) = self.db.as_ref() {
                    match (db.report_tables(), db.report_templates()) {
                        (Ok(tables), Ok(templates)) => {
                            self.report_workshop.tables = tables;
                            self.report_workshop.templates = templates;
                            match db.report_runs() {
                                Ok(runs) => self.report_workshop.runs = runs,
                                Err(e) => self.report_workshop.error = e.to_string(),
                            }
                        }
                        (Err(e), _) | (_, Err(e)) => self.report_workshop.error = e.to_string(),
                    }
                }
            }
        }
    }

    pub(super) fn render_report_workshop_dialog(&mut self, ctx: &egui::Context) {
        if !self.report_workshop.open {
            return;
        }
        let database_path = self.data_dir.join("desk.db");
        let state = &mut self.report_workshop;
        let db = &mut self.db;
        let dialog = centered_dialog(
            ctx,
            "report-workshop",
            "数据建模与报表工坊",
            "",
            900.,
            |ui| {
                ui.horizontal(|ui| {
                    for (i, label) in ["导入工作簿", "管理数据表", "设计报表"].iter().enumerate()
                    {
                        ui.selectable_value(&mut state.page, i, *label);
                    }
                });
                if !state.notice.is_empty() {
                    ui.label(&state.notice);
                }
                if !state.error.is_empty() {
                    ui.colored_label(Color32::from_rgb(184, 65, 71), &state.error);
                }
                egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 190.).max(250.))
                    .show(ui, |ui| {
                        if state.page==0 {
                        ui.heading("1. 导入工作簿");
                        if ui
                            .add_enabled(!state.busy(), egui::Button::new("选择 Excel 工作簿…"))
                            .clicked()
                            && let Some(path) = rfd::FileDialog::new()
                                .add_filter("Excel", &["xlsx", "xlsm", "xls", "xlsb", "ods"])
                                .pick_file()
                        {
                            state.inspect(path);
                        }
                        if let Some(path) = &state.path {
                            ui.label(path.display().to_string());
                        }
                        if !state.sheets.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                for (i, sheet) in state.sheets.iter().enumerate() {
                                    if ui
                                        .selectable_label(state.active == i, &sheet.preview.name)
                                        .clicked()
                                    {
                                        state.active = i;
                                    }
                                }
                            });
                            let draft = &mut state.sheets[state.active];
                            ui.horizontal_wrapped(|ui| {
                                ui.label("已有数据表");
                                choose(
                                    ui,
                                    "existing-table",
                                    &mut draft.plan.table,
                                    state.tables.iter().map(|p| p.table.clone()),
                                );
                                if ui.button("加载已有字段映射").clicked()
                                    && let Some(existing) =
                                        state.tables.iter().find(|p| p.table == draft.plan.table)
                                {
                                    draft.plan.fields = existing.fields.clone();
                                }
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.checkbox(&mut draft.enabled, "导入此 Sheet");
                                ui.label("目标表名");
                                ui.add(egui::TextEdit::singleline(&mut draft.plan.table).desired_width(150.));
                                ui.label("数据起始行");
                                ui.add(
                                    egui::DragValue::new(&mut draft.plan.first_data_row)
                                        .range(1..=1_048_576),
                                );
                                ui.label("A 列结束标记（可选）");
                                ui.add(egui::TextEdit::singleline(&mut draft.plan.end_marker).desired_width(150.));
                                let mut skip=draft.plan.skip_empty_column.is_some();
                                if ui.checkbox(&mut skip,"跳过关键列为空的行").changed(){draft.plan.skip_empty_column=if skip {Some(0)} else {None};}
                                if let Some(col)=&mut draft.plan.skip_empty_column {ui.label("关键列号");let mut one=*col+1;if ui.add(egui::DragValue::new(&mut one).range(1..=16_384)).changed(){*col=one-1;}}
                            });
                            ui.horizontal(|ui| {
                                ui.label("用哪一行作为字段名");
                                ui.add(egui::DragValue::new(&mut draft.header_row).range(1..=30));
                                if ui.button("读取表头").clicked()
                                    && let Some(row) = draft.preview.rows.get(draft.header_row - 1)
                                    {
                                        for (i, field) in draft.plan.fields.iter_mut().enumerate() {
                                            if let Source::Column(col) = field.source {
                                                if let Some(name) =
                                                    row.get(col).filter(|s| !s.trim().is_empty())
                                                {
                                                    field.name = name.clone();
                                                } else {
                                                    field.name = format!("字段{}", i + 1);
                                                }
                                            }
                                        }
                                }
                            });
                            egui::CollapsingHeader::new("字段映射")
                                .default_open(true)
                                .show(ui, |ui| {
                                    let mut remove = None;
                                    for (i, field) in draft.plan.fields.iter_mut().enumerate() {
                                        ui.push_id(i, |ui| {
                                            ui.horizontal_wrapped(|ui| {
                                                let mut fixed_value =
                                                    matches!(field.source, Source::Constant(_));
                                                if ui.checkbox(&mut fixed_value, "固定值").changed() {
                                                    field.source = if fixed_value {
                                                        Source::Constant(String::new())
                                                    } else {
                                                        Source::Column(0)
                                                    };
                                                }
                                                match &mut field.source {
                                                    Source::Column(col) => {
                                                        ui.label("Excel 列号");
                                                        let mut one = *col + 1;
                                                        if ui
                                                            .add(
                                                                egui::DragValue::new(&mut one)
                                                                    .range(1..=16_384),
                                                            )
                                                            .changed()
                                                        {
                                                            *col = one - 1;
                                                        }
                                                    }
                                                    Source::Constant(value) => {
                                                        ui.add(egui::TextEdit::singleline(value).desired_width(150.));
                                                    }
                                                }
                                                ui.add(egui::TextEdit::singleline(&mut field.name).desired_width(150.));
                                                let mut kind = match field.kind {
                                                    Kind::Text => 0,
                                                    Kind::Integer => 1,
                                                    Kind::Real => 2,
                                                    Kind::Decimal => 3,
                                                };
                                                egui::ComboBox::from_id_salt("kind")
                                                    .selected_text(
                                                        ["文本", "整数", "浮点", "精确小数"][kind],
                                                    )
                                                    .show_ui(ui, |ui| {
                                                        for (n, label) in
                                                            ["文本", "整数", "浮点", "精确小数"]
                                                                .iter()
                                                                .enumerate()
                                                        {
                                                            ui.selectable_value(
                                                                &mut kind, n, *label,
                                                            );
                                                        }
                                                    });
                                                field.kind = match kind {
                                                    0 => Kind::Text,
                                                    1 => Kind::Integer,
                                                    2 => Kind::Real,
                                                    _ => Kind::Decimal,
                                                };
                                                ui.checkbox(&mut field.required, "必填");
                                                if ui.small_button("移除").clicked() {
                                                    remove = Some(i);
                                                }
                                            });
                                        });
                                    }
                                    if let Some(i) = remove {
                                        draft.plan.fields.remove(i);
                                    }
                                    if ui.button("增加固定字段").clicked() {
                                        draft.plan.fields.push(Field {
                                            name: String::new(),
                                            kind: Kind::Text,
                                            source: Source::Constant(String::new()),
                                            required: false,
                                        });
                                    }
                                });
                            egui::CollapsingHeader::new("源文件前 30 行预览").show(ui, |ui| {
                                for (i, row) in draft.preview.rows.iter().enumerate() {
                                    ui.label(format!("{}: {}", i + 1, row.join(" | ")));
                                }
                            });
                            ui.horizontal_wrapped(|ui| {
                                ui.selectable_value(&mut state.mode, ImportMode::Create, "新建表");
                                ui.selectable_value(&mut state.mode, ImportMode::Append, "追加");
                                ui.selectable_value(
                                    &mut state.mode,
                                    ImportMode::ReplaceAll,
                                    "整表替换",
                                );
                            });
                            if state.mode == ImportMode::ReplaceAll {
                                ui.checkbox(
                                    &mut state.confirm_replace,
                                    "确认清空所有选中的目标表后重新导入",
                                );
                            } else {
                                state.confirm_replace = false;
                            }
                            if ui
                                .add_enabled(
                                    !state.busy()
                                        && (state.mode != ImportMode::ReplaceAll
                                            || state.confirm_replace),
                                    egui::Button::new("执行导入"),
                                )
                                .clicked()
                            {
                                let plans = state
                                    .sheets
                                    .iter()
                                    .filter(|s| s.enabled)
                                    .map(|s| s.plan.clone())
                                    .collect::<Vec<_>>();
                                if let Some(path) = state.path.clone() {
                                    state.import(database_path.clone(), path, plans, state.mode);
                                    state.confirm_replace = false;
                                }
                            }
                        }
                        }
                        if state.page==1 {
                        ui.separator();
                        ui.heading("2. 管理数据表");
                        ui.horizontal_wrapped(|ui| {
                            ui.label("数据表");
                            choose(
                                ui,
                                "manage-table",
                                &mut state.data_table,
                                state.tables.iter().map(|p| p.table.clone()),
                            );
                            if ui.button("读取数据").clicked()
                                && let Some(db) = db.as_ref()
                            {
                                match db.report_read_rows(&state.data_table, state.data_offset) {
                                    Ok((rev, rows)) => {
                                        state.data_revision = rev;
                                        state.data_rows = rows;
                                        state.data_loaded_table = state.data_table.clone();
                                        state.error.clear();
                                    }
                                    Err(e) => state.error = e.to_string(),
                                }
                            }
                            if ui.button("上一页").clicked() {
                                state.data_offset = state.data_offset.saturating_sub(100);
                                state.data_rows.clear();
                            }
                            if ui.button("下一页").clicked() {
                                state.data_offset = state.data_offset.saturating_add(100);
                                state.data_rows.clear();
                            }
                            ui.label(format!("从第 {} 条起", state.data_offset + 1));
                        });
                        let data_fields = state
                            .tables
                            .iter()
                            .find(|p| p.table == state.data_table)
                            .map(|p| p.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>())
                            .unwrap_or_default();
                        egui::CollapsingHeader::new("查看当前页").show(ui, |ui| {
                            if state.data_loaded_table == state.data_table {
                                for (id, values) in &state.data_rows {
                                    ui.label(format!(
                                        "行 {id}：{}",
                                        values
                                            .iter()
                                            .map(|v| v.as_deref().unwrap_or("<空>"))
                                            .collect::<Vec<_>>()
                                            .join(" | ")
                                    ));
                                }
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("编辑行号");
                            ui.add(egui::TextEdit::singleline(&mut state.row_id).desired_width(150.));
                            ui.label("字段");
                            choose(ui, "edit-field", &mut state.edit_field, data_fields.clone());
                            ui.label("新值");
                            ui.add(egui::TextEdit::singleline(&mut state.edit_value).desired_width(150.));
                            if ui
                                .add_enabled(!state.busy(), egui::Button::new("保存单元格"))
                                .clicked()
                                && let Some(db) = db.as_mut()
                            {
                                if state.data_loaded_table != state.data_table {
                                    state.error = "请先读取当前数据表".into();
                                } else {
                                    match state.row_id.parse::<i64>() {
                                        Ok(id)
                                            if state
                                                .data_rows
                                                .iter()
                                                .any(|(row, _)| *row == id) =>
                                        {
                                            match db.report_update_cell(
                                                &state.data_table,
                                                id,
                                                &state.edit_field,
                                                &state.edit_value,
                                                state.data_revision,
                                            ) {
                                                Ok(()) => {
                                                    state.notice =
                                                        "数据已保存，请重新读取当前页".into();
                                                    state.data_rows.clear();
                                                    state.data_loaded_table.clear();
                                                    state.result.clear();
                                                    state.edit_value.clear();
                                                    state.error.clear();
                                                }
                                                Err(e) => state.error = e.to_string(),
                                            }
                                        }
                                        _ => state.error = "请输入当前页显示的行号".into(),
                                    }
                                }
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("新增字段");
                            ui.add(egui::TextEdit::singleline(&mut state.new_field).desired_width(150.));
                            egui::ComboBox::from_id_salt("new-field-kind")
                                .selected_text(["文本", "整数", "浮点", "精确小数"][state.new_kind])
                                .show_ui(ui, |ui| {
                                    for (i, name) in
                                        ["文本", "整数", "浮点", "精确小数"].iter().enumerate()
                                    {
                                        ui.selectable_value(&mut state.new_kind, i, *name);
                                    }
                                });
                            if ui
                                .add_enabled(!state.busy(), egui::Button::new("添加字段"))
                                .clicked()
                                && let Some(db) = db.as_mut()
                            {
                                let kind = match state.new_kind {
                                    0 => Kind::Text,
                                    1 => Kind::Integer,
                                    2 => Kind::Real,
                                    _ => Kind::Decimal,
                                };
                                let field = Field {
                                    name: state.new_field.clone(),
                                    kind,
                                    source: Source::Constant(String::new()),
                                    required: false,
                                };
                                match db.report_add_field(&state.data_table, field) {
                                    Ok(()) => {
                                        state.tables = db.report_tables().unwrap_or_default();
                                        state.data_rows.clear();
                                        state.result.clear();
                                        state.new_field.clear();
                                        state.notice = "字段已添加，请为旧数据补充取值".into();
                                        state.error.clear();
                                    }
                                    Err(e) => state.error = e.to_string(),
                                }
                            }
                        });
                        ui.horizontal_wrapped(|ui| {
                            ui.label("修改字段名");choose(ui,"rename-field",&mut state.edit_field,data_fields.clone());ui.label("新名称");ui.add(egui::TextEdit::singleline(&mut state.rename_to).desired_width(150.));
                            if ui.add_enabled(!state.busy(),egui::Button::new("保存字段名称")).clicked() && let Some(db)=db.as_mut(){match db.report_rename_field(&state.data_table,&state.edit_field,&state.rename_to){Ok(())=>{state.tables=db.report_tables().unwrap_or_default();state.data_rows.clear();state.data_loaded_table.clear();state.result.clear();state.edit_field=state.rename_to.clone();state.rename_to.clear();state.notice="字段名称已修改".into();state.error.clear();},Err(e)=>state.error=e.to_string()}}
                        });
                        egui::CollapsingHeader::new("按规则批量刷字段").show(ui,|ui|{
                            ui.label("目标字段使用上方选中的字段；规则逐行计算，任一行出错整批回滚。");
                            if ui.button("加载此字段上次刷数规则").clicked() && let Some(db)=db.as_ref(){match db.report_field_rule(&state.data_table,&state.edit_field){Ok(Some(FieldRule::Constant(value)))=>{state.refresh_constant=true;state.refresh_text=value;state.error.clear();},Ok(Some(FieldRule::Calculation(rule)))=>{state.refresh_constant=false;state.refresh_rule=rule;state.error.clear();},Ok(None)=>state.error="该字段没有已保存的规则".into(),Err(e)=>state.error=e.to_string()}}
                            ui.checkbox(&mut state.refresh_constant,"统一填入固定值");
                            if state.refresh_constant {ui.label("固定值");ui.add(egui::TextEdit::singleline(&mut state.refresh_text).desired_width(180.));}else{edit_row_rule(ui,&mut state.refresh_rule,&data_fields,"refresh-root".into(),0);}
                            if ui.add_enabled(!state.busy(),egui::Button::new("保存刷数规则")).clicked() && let Some(db)=db.as_mut(){let rule=if state.refresh_constant{FieldRule::Constant(state.refresh_text.clone())}else{FieldRule::Calculation(state.refresh_rule.clone())};match db.report_save_field_rule(&state.data_table,&state.edit_field,&rule){Ok(())=>{state.notice="刷数规则已保存".into();state.error.clear();},Err(e)=>state.error=e.to_string()}}
                            ui.checkbox(&mut state.confirm_refresh,"确认更新当前数据表所有行的目标字段");
                            if ui.add_enabled(!state.busy()&&state.confirm_refresh,egui::Button::new("执行规则刷数")).clicked(){
                                if state.data_loaded_table!=state.data_table {state.error="请先读取当前数据表以确认版本".into();}
                                else {state.refresh(database_path.clone(),state.data_table.clone(),state.edit_field.clone(),state.refresh_rule.clone(),state.data_revision,if state.refresh_constant{Some(state.refresh_text.clone())}else{None});state.confirm_refresh=false;}
                            }
                        });
                        }
                        if state.page==2 {
                        ui.separator();
                        ui.heading("3. 报表规则与动态行");
                        ui.horizontal_wrapped(|ui| {
                            ui.label("模板编号");
                            ui.add(egui::TextEdit::singleline(&mut state.template_id).desired_width(150.));
                            ui.label("模板名称");
                            ui.add(egui::TextEdit::singleline(&mut state.template.title).desired_width(150.));
                            ui.label("动态行来源表");
                            choose(
                                ui,
                                "group-table",
                                &mut state.template.group_table,
                                state.tables.iter().map(|p| p.table.clone()),
                            );
                            let fields = state
                                .tables
                                .iter()
                                .find(|p| p.table == state.template.group_table)
                                .map(|p| {
                                    p.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>()
                                })
                                .unwrap_or_default();
                            ui.label("分组字段");
                            choose(ui, "group-field", &mut state.template.group_field, fields);
                        });
                        let group_fields = state
                            .tables
                            .iter()
                            .find(|p| p.table == state.template.group_table)
                            .map(|p| p.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>())
                            .unwrap_or_default();
                        let mut remove_filter = None;
                        for (i, (field, value)) in
                            state.template.group_filters.iter_mut().enumerate()
                        {
                            ui.horizontal(|ui| {
                                ui.label("动态行筛选");
                                choose(ui, ("group-filter", i), field, group_fields.clone());
                                ui.label("等于");
                                ui.add(egui::TextEdit::singleline(value).desired_width(150.));
                                if ui.small_button("移除").clicked() {
                                    remove_filter = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove_filter {
                            state.template.group_filters.remove(i);
                        }
                        if ui.button("增加动态行筛选").clicked() {
                            state
                                .template
                                .group_filters
                                .push((String::new(), String::new()));
                        }
                        let mut remove = None;
                        for (i, (name, rule)) in state.template.columns.iter_mut().enumerate() {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(format!("结果列 {}", i + 1));
                                    ui.add(egui::TextEdit::singleline(name).desired_width(150.));
                                    if ui.small_button("移除").clicked() {
                                        remove = Some(i);
                                    }
                                });
                                edit_rule(ui, rule, &state.tables, format!("rule-{i}"), 0);
                            });
                        }
                        if let Some(i) = remove {
                            state.template.columns.remove(i);
                        }
                        if ui.button("添加计算列").clicked() {
                            state
                                .template
                                .columns
                                .push(("新列".into(), Rule::Number("0".into())));
                        }
                        ui.horizontal_wrapped(|ui| {
                            ui.label("排序列");
                            egui::ComboBox::from_id_salt("report-sort").selected_text(state.template.sort_column.and_then(|i|state.template.columns.get(i).map(|c|c.0.as_str())).unwrap_or("按分组字段")).show_ui(ui,|ui|{
                                ui.selectable_value(&mut state.template.sort_column,None,"按分组字段");
                                for (i,(name,_)) in state.template.columns.iter().enumerate(){ui.selectable_value(&mut state.template.sort_column,Some(i),name);}
                            });
                            ui.checkbox(&mut state.template.descending,"数值降序");
                            let mut limited=state.template.limit.is_some();
                            if ui.checkbox(&mut limited,"限制行数").changed(){state.template.limit=if limited{Some(10)}else{None};}
                            if let Some(limit)=&mut state.template.limit{ui.add(egui::DragValue::new(limit).range(1..=1_000_000));}
                        });
                        egui::CollapsingHeader::new("逐格设置固定位置").show(ui,|ui|{
                            ui.horizontal(|ui|{ui.label("动态数据起始行");let mut row=state.template.dynamic_row+1;if ui.add(egui::DragValue::new(&mut row).range(3..=1_048_576)).changed(){state.template.dynamic_row=row-1;}ui.label("分组起始列");let mut col=state.template.group_column as u32+1;if ui.add(egui::DragValue::new(&mut col).range(1..=16_384)).changed(){state.template.group_column=(col-1) as u16;}});
                            ui.label("动态行之后的自定义单元格会随展开行数下移。");
                            ui.label("点击网格中的空格可新增自定义单元格；更多行列可以在下方直接输入。");
                            let mut selected=None;
                            egui::ScrollArea::horizontal().id_salt("template-grid-scroll").show(ui,|ui| {
                                egui::Grid::new("template-grid").striped(true).show(ui,|ui| {
                                    ui.label("行/列");for col in 0..8 {ui.label(((b'A'+col as u8) as char).to_string());}ui.end_row();
                                    for row in 0..12u32 {ui.label((row+1).to_string());for col in 0..8u16 {
                                        let fixed=state.template.cells.iter().find(|cell|cell.row==row && cell.column==col);
                                        let within=col>=state.template.group_column && (col as usize)<=state.template.group_column as usize+state.template.columns.len();
                                        let reserved=(row==0 && col==0)||((row+1==state.template.dynamic_row||row==state.template.dynamic_row)&&within);
                                        let label=if reserved {if row==0 {"标题".to_string()} else if row+1==state.template.dynamic_row {"表头".to_string()} else {"动态".to_string()}} else if let Some(cell)=fixed {match &cell.value {CellValue::Text(value)=>value.chars().take(5).collect(),CellValue::Calculated{..}=>"计算".into()}} else {"＋".into()};
                                        if ui.add_enabled(!reserved,egui::Button::new(label).min_size(egui::vec2(52.,22.))).clicked(){selected=Some((row,col));}
                                    }ui.end_row();}
                                });
                            });
                            if let Some((row,column))=selected && !state.template.cells.iter().any(|cell|cell.row==row && cell.column==column){state.template.cells.push(TemplateCell{row,column,value:CellValue::Text(String::new())});}
                            let mut remove=None;
                            for (i,cell) in state.template.cells.iter_mut().enumerate(){ui.group(|ui|{
                                ui.horizontal_wrapped(|ui|{
                                    ui.label("行");let mut row=cell.row+1;if ui.add(egui::DragValue::new(&mut row).range(1..=1_048_576)).changed(){cell.row=row-1;}
                                    ui.label("列");let mut column=cell.column as u32+1;if ui.add(egui::DragValue::new(&mut column).range(1..=16_384)).changed(){cell.column=(column-1) as u16;}
                                    let mut calculated=matches!(cell.value,CellValue::Calculated{..});
                                    if ui.checkbox(&mut calculated,"计算值").changed(){cell.value=if calculated{CellValue::Calculated{group:String::new(),rule:Rule::Number("0".into())}}else{CellValue::Text(String::new())};}
                                    if ui.button("移除单元格").clicked(){remove=Some(i);}
                                });
                                match &mut cell.value {CellValue::Text(text)=>{ui.label("固定文字");ui.add(egui::TextEdit::singleline(text).desired_width(150.));},CellValue::Calculated{group,rule}=>{ui.horizontal(|ui|{ui.label("分组匹配值");ui.add(egui::TextEdit::singleline(group).desired_width(150.));});edit_rule(ui,rule,&state.tables,format!("cell-{i}"),0);}}
                            });}
                            if let Some(i)=remove{state.template.cells.remove(i);}
                            if ui.button("增加单元格").clicked(){state.template.cells.push(TemplateCell{row:3,column:0,value:CellValue::Text(String::new())});}
                        });
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .add_enabled(!state.busy(), egui::Button::new("保存新版本"))
                                .clicked()
                                && let Some(db) = db.as_mut()
                            {
                                match db.report_save_template(&state.template_id, &state.template) {
                                    Ok(version) => {
                                        state.saved_template=Some((state.template_id.clone(),version,serde_json::to_string(&state.template).unwrap_or_default()));
                                        state.notice = format!("已保存版本 {version}");
                                        state.error.clear();
                                        state.templates = db.report_templates().unwrap_or_default();
                                    }
                                    Err(e) => state.error = e.to_string(),
                                }
                            }
                            if ui
                                .add_enabled(!state.busy(), egui::Button::new("生成预览"))
                                .clicked()
                            {
                                state.result.clear();
                                state.generate(database_path.clone(), state.template.clone());
                            }
                            if ui
                                .add_enabled(!state.busy(), egui::Button::new("导出 Excel…"))
                                .clicked()
                                && let Some(path) = rfd::FileDialog::new()
                                    .set_file_name("报表.xlsx")
                                    .add_filter("Excel", &["xlsx"])
                                    .save_file()
                            {
                                if state.result.is_empty()
                                    || state.result_signature
                                        != serde_json::to_string(&state.template)
                                            .unwrap_or_default()
                                    || db
                                        .as_ref()
                                        .map(|db| db.report_revision())
                                        .transpose()
                                        .ok()
                                        .flatten()
                                        != Some(state.result_revision)
                                {
                                    state.error = "模板或数据已变化，请重新生成预览".into();
                                } else if state.saved_template.as_ref().is_none_or(|(id,_,saved)|id!=&state.template_id || saved!=&state.result_signature) {
                                    state.error="请先保存当前模板版本后再导出".into();
                                } else {
                                    match report_workshop::export_report(
                                        &path,
                                        &state.template,
                                        &state.result,
                                        &state.result_cells,
                                    ) {
                                        Ok(()) => {
                                            let (id,version,_)=state.saved_template.as_ref().expect("saved template checked");
                                            match db.as_mut().map(|db|db.report_record_export(id,*version,&state.template,state.result_revision,&path)) {
                                                Some(Ok(()))=>{state.notice = format!("已导出 {}，并记录模板版本与数据版本", path.display());if let Some(db)=db.as_ref(){match db.report_runs(){Ok(runs)=>{state.runs=runs;state.error.clear();},Err(e)=>state.error=e.to_string()}}},
                                                Some(Err(e))=>{state.error=format!("文件已写入 {}，但生成记录保存失败：{e}",path.display());},
                                                None=>{state.error=format!("文件已写入 {}，但数据库不可用，未记录生成历史",path.display());}
                                            }
                                        }
                                        Err(e) => state.error = e.to_string(),
                                    }
                                }
                            }
                        });
                        egui::CollapsingHeader::new("历史模板版本").show(ui, |ui| {
                            for (id, version, template) in &state.templates {
                                ui.horizontal(|ui| {
                                    if ui.button(format!("加载 {id} · 第 {version} 版")).clicked() {
                                        let current=serde_json::to_string(&state.template).unwrap_or_default();
                                        let dirty=state.saved_template.as_ref().map(|(saved_id,_,saved)|saved_id!=&state.template_id || saved!=&current).unwrap_or(!state.template.columns.is_empty()||!state.template.cells.is_empty()||!state.template_id.is_empty()||state.template.title!="新报表"||!state.template.group_table.is_empty()||!state.template.group_field.is_empty());
                                        if dirty {state.pending_load=Some((id.clone(),*version,template.clone()));}else{state.template=template.clone();state.template_id=id.clone();state.saved_template=Some((id.clone(),*version,serde_json::to_string(template).unwrap_or_default()));state.result.clear();}
                                    }
                                    if ui.button("加入工作簿").clicked() {
                                        if state.workbook_sheets.iter().any(|(old,v,_)|old==id && v==version){state.error="此模板版本已加入工作簿".into();}
                                        else {state.workbook_sheets.push((id.clone(),*version,template.clone()));state.error.clear();}
                                    }
                                });
                            }
                        });
                        if state.pending_load.is_some() {
                            ui.colored_label(Color32::from_rgb(184,65,71),"当前模板有未保存修改。请先保存，或明确放弃修改后再加载。");
                            ui.horizontal(|ui| {
                                if ui.button("放弃修改并加载").clicked() && let Some((id,version,template))=state.pending_load.take() {
                                    state.template_id=id.clone();state.saved_template=Some((id,version,serde_json::to_string(&template).unwrap_or_default()));state.template=template;state.result.clear();
                                }
                                if ui.button("取消加载").clicked(){state.pending_load=None;}
                            });
                        }
                        egui::CollapsingHeader::new("多工作表工作簿").show(ui,|ui| {
                            ui.label("从历史模板中选择多个已保存版本，按顺序生成一个 Excel 工作簿。");
                            let mut remove=None;for (i,(id,version,template)) in state.workbook_sheets.iter().enumerate(){ui.horizontal(|ui|{ui.label(format!("{}：{} · {id} 第 {version} 版",i+1,template.title));if ui.button("移出").clicked(){remove=Some(i);}});}if let Some(i)=remove{state.workbook_sheets.remove(i);}
                            if ui.add_enabled(!state.busy() && !state.workbook_sheets.is_empty(),egui::Button::new("生成多工作表 Excel…")).clicked() && let Some(path)=rfd::FileDialog::new().set_file_name("多工作表报表.xlsx").add_filter("Excel",&["xlsx"]).save_file(){state.export_workbook(database_path.clone(),path,state.workbook_sheets.clone());}
                        });
                        egui::CollapsingHeader::new("报表生成历史").show(ui,|ui|{for (id,version,revision,path,hash,time) in &state.runs {ui.label(format!("{time} · {id} 第 {version} 版 · 数据版本 {revision}\n{path}\nSHA-256: {hash}"));}});
                        if !state.result.is_empty()
                            && state.result_signature
                                == serde_json::to_string(&state.template).unwrap_or_default()
                        {
                            egui::Grid::new("report-preview")
                                .striped(true)
                                .show(ui, |ui| {
                                    ui.label(&state.template.group_field);
                                    for (name, _) in &state.template.columns {
                                        ui.label(name);
                                    }
                                    ui.end_row();
                                    for (group, values) in state.result.iter().take(100) {
                                        ui.label(group);
                                        for value in values {
                                            ui.label(value.to_string());
                                        }
                                        ui.end_row();
                                    }
                                });
                        }
                        }
                    });
            },
        );
        if dialog_closed(&dialog) {
            state.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::editor_tests::{input_frame, pointer};
    fn position(output: &egui::FullOutput, text: &str) -> Option<egui::Pos2> {
        output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if t.galley.text() == text => Some(t.pos + t.galley.size() * 0.5),
            _ => None,
        })
    }
    #[test]
    fn entry_opens_and_all_three_tabs_are_reachable() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = PersonalDeskApp::open_data_directory(dir.path().join("data"));
        app.section = Section::Tools;
        app.search_query = "报表".into();
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        input_frame(&mut app, &ctx, 0.0, vec![], vec![]);
        let output = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
        let p = position(&output, "数据建模与报表工坊").expect("missing tool");
        input_frame(&mut app, &ctx, 0.2, pointer(p, true), vec![]);
        input_frame(&mut app, &ctx, 0.3, pointer(p, false), vec![]);
        assert!(app.report_workshop.open);
        let output = input_frame(&mut app, &ctx, 0.4, vec![], vec![]);
        let p = position(&output, "设计报表").expect("missing tab");
        input_frame(&mut app, &ctx, 0.5, pointer(p, true), vec![]);
        input_frame(&mut app, &ctx, 0.6, pointer(p, false), vec![]);
        assert_eq!(app.report_workshop.page, 2);
        let output = input_frame(&mut app, &ctx, 0.7, vec![], vec![]);
        assert!(position(&output, "添加计算列").is_some());
    }
}
