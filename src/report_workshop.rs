//! Data import and report rules for the report workshop.
mod xlsx_text;
use std::{collections::HashSet, io::Read, path::Path, str::FromStr};

use calamine::{Data, Reader, open_workbook_auto};
use rusqlite::{Connection, params_from_iter, types::Value};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{DeskError, Result};
pub type DataPage = (i64, Vec<(i64, Vec<Option<String>>)>);
pub type SheetExport<'a> = (
    &'a Template,
    &'a [(String, Vec<Decimal>)],
    &'a [ResolvedCell],
);
pub type RunInfo = (String, i64, i64, String, String, String);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Source {
    /// Zero-based absolute worksheet column, independent of the range origin.
    Column(usize),
    Constant(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Kind {
    Text,
    Integer,
    Real,
    /// Decimal is stored as text to preserve its exact value.
    Decimal,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub kind: Kind,
    pub source: Source,
    pub required: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SheetImport {
    pub sheet: String,
    pub table: String,
    /// One-based first data row, after titles, headers and any placeholder rows.
    pub first_data_row: usize,
    #[serde(default)]
    pub end_marker: String,
    #[serde(default)]
    pub skip_empty_column: Option<usize>,
    pub fields: Vec<Field>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportMode {
    Create,
    Append,
    ReplaceAll,
}

#[derive(Clone, Debug)]
pub struct SheetPreview {
    pub name: String,
    pub rows: Vec<Vec<String>>,
    pub width: usize,
}

pub fn inspect_workbook(path: &Path) -> Result<Vec<SheetPreview>> {
    let mut book = open_workbook_auto(path)
        .map_err(|e| DeskError::InvalidInput(format!("无法打开工作簿: {e}")))?;
    let names = book.sheet_names().to_vec();
    let text_cells = xlsx_text::read(path, &names)?;
    let mut previews = Vec::new();
    for name in names {
        let mut range = book
            .worksheet_range(&name)
            .map_err(|e| DeskError::InvalidInput(format!("读取工作表 {name} 失败: {e}")))?;
        if let Some(cells) = text_cells.get(&name) {
            xlsx_text::apply(&mut range, cells);
        }
        let (start_row, start_col) = range.start().unwrap_or((0, 0));
        let width = start_col as usize + range.width();
        if width > 16384 {
            return bad("工作表列数超过 Excel 的限制");
        }
        let mut rows = vec![Vec::new(); (start_row as usize).min(30)];
        for row in range.rows().take(30usize.saturating_sub(rows.len())) {
            let mut cells = vec![String::new(); start_col as usize];
            cells.extend(row.iter().map(ToString::to_string));
            rows.push(cells);
        }
        previews.push(SheetPreview { name, rows, width });
    }
    Ok(previews)
}

pub fn file_hash(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// A structured rule; names are quoted and values passed as parameters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Rule {
    Number(String),
    Sum {
        table: String,
        field: String,
        group_field: String,
        filters: Vec<(String, String)>,
        #[serde(default)]
        all_rows: bool,
    },
    LinkedSum {
        left_table: String,
        right_table: String,
        amount_field: String,
        group_field: String,
        keys: Vec<(String, String)>,
        right_filters: Vec<(String, String)>,
    },
    Add(Box<Rule>, Box<Rule>),
    Subtract(Box<Rule>, Box<Rule>),
    Multiply(Box<Rule>, Box<Rule>),
    Divide(Box<Rule>, Box<Rule>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RowRule {
    Field(String),
    Number(String),
    Add(Box<RowRule>, Box<RowRule>),
    Subtract(Box<RowRule>, Box<RowRule>),
    Multiply(Box<RowRule>, Box<RowRule>),
    Divide(Box<RowRule>, Box<RowRule>),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FieldRule {
    Constant(String),
    Calculation(RowRule),
}

fn persist_field_rule(
    tx: &rusqlite::Transaction<'_>,
    table: &str,
    field: &str,
    rule: &FieldRule,
) -> Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS report_workshop_field_rules(table_name TEXT NOT NULL,field_name TEXT NOT NULL,rule TEXT NOT NULL,PRIMARY KEY(table_name,field_name))")?;
    tx.execute("INSERT INTO report_workshop_field_rules(table_name,field_name,rule) VALUES(?1,?2,?3) ON CONFLICT(table_name,field_name) DO UPDATE SET rule=excluded.rule",rusqlite::params![table,field,serde_json::to_string(rule)?])?;
    Ok(())
}

pub fn save_field_rule(
    db: &mut Connection,
    table: &str,
    target: &str,
    rule: &FieldRule,
) -> Result<()> {
    require_columns(db, table, &[target])?;
    fn check(db: &Connection, table: &str, rule: &RowRule, depth: usize) -> Result<()> {
        if depth > 64 {
            return bad("规则嵌套过深");
        }
        match rule {
            RowRule::Field(name) => require_columns(db, table, &[name])?,
            RowRule::Number(value) => {
                value.parse::<Decimal>()?;
            }
            RowRule::Add(a, b)
            | RowRule::Subtract(a, b)
            | RowRule::Multiply(a, b)
            | RowRule::Divide(a, b) => {
                check(db, table, a, depth + 1)?;
                check(db, table, b, depth + 1)?;
            }
        }
        Ok(())
    }
    match rule {
        FieldRule::Calculation(expr) => check(db, table, expr, 0)?,
        FieldRule::Constant(text) => {
            let raw: String = db.query_row(
                "SELECT plan FROM report_workshop_tables WHERE name=?1",
                [table],
                |r| r.get(0),
            )?;
            let plan: SheetImport = serde_json::from_str(&raw)?;
            let mut field = plan
                .fields
                .into_iter()
                .find(|f| f.name == target)
                .ok_or_else(|| DeskError::InvalidInput("字段不存在".into()))?;
            field.source = Source::Constant(text.clone());
            convert(None, &field, 0)?;
        }
    }
    let tx = db.transaction()?;
    persist_field_rule(&tx, table, target, rule)?;
    tx.commit()?;
    Ok(())
}

pub fn fill_field(
    db: &mut Connection,
    table: &str,
    target: &str,
    text: &str,
    expected_revision: i64,
) -> Result<usize> {
    require_columns(db, table, &[target])?;
    let tx = db.transaction()?;
    let revision: i64 = tx.query_row(
        "SELECT value FROM report_workshop_meta WHERE key='data_revision'",
        [],
        |r| r.get(0),
    )?;
    if revision != expected_revision {
        return Err(DeskError::ConcurrentModification);
    }
    let raw: String = tx.query_row(
        "SELECT plan FROM report_workshop_tables WHERE name=?1",
        [table],
        |r| r.get(0),
    )?;
    let plan: SheetImport = serde_json::from_str(&raw)?;
    let mut field = plan
        .fields
        .into_iter()
        .find(|f| f.name == target)
        .ok_or_else(|| DeskError::InvalidInput("目标字段不存在".into()))?;
    field.source = Source::Constant(text.into());
    let value = convert(None, &field, 0)?;
    let count = tx.execute(
        &format!("UPDATE {} SET {}=?1", quote(table)?, quote(target)?),
        [value],
    )?;
    if count == 0 {
        return bad("数据表为空，无法刷数");
    }
    persist_field_rule(&tx, table, target, &FieldRule::Constant(text.into()))?;
    tx.execute(
        "UPDATE report_workshop_meta SET value=value+1 WHERE key='data_revision'",
        [],
    )?;
    tx.commit()?;
    Ok(count)
}

/// Recalculate an existing field from fields in the same row, atomically.
pub fn refresh_field(
    db: &mut Connection,
    table: &str,
    target: &str,
    rule: &RowRule,
    expected_revision: i64,
) -> Result<usize> {
    require_columns(db, table, &[target])?;
    let tx = db.transaction()?;
    let revision: i64 = tx.query_row(
        "SELECT value FROM report_workshop_meta WHERE key='data_revision'",
        [],
        |r| r.get(0),
    )?;
    if revision != expected_revision {
        return Err(DeskError::ConcurrentModification);
    }
    let raw: String = tx.query_row(
        "SELECT plan FROM report_workshop_tables WHERE name=?1",
        [table],
        |r| r.get(0),
    )?;
    let plan: SheetImport = serde_json::from_str(&raw)?;
    let target_field = plan
        .fields
        .iter()
        .find(|f| f.name == target)
        .ok_or_else(|| DeskError::InvalidInput("目标字段不存在".into()))?;
    let names = plan
        .fields
        .iter()
        .map(|f| quote(&f.name).map(|n| format!("CAST({n} AS TEXT)")))
        .collect::<Result<Vec<_>>>()?
        .join(",");
    let mut stmt = tx.prepare(&format!(
        "SELECT rowid,{names} FROM {} ORDER BY rowid",
        quote(table)?
    ))?;
    let source = stmt
        .query_map([], |r| {
            let mut values = Vec::new();
            for i in 0..plan.fields.len() {
                values.push(r.get::<_, Option<String>>(i + 1)?);
            }
            Ok((r.get::<_, i64>(0)?, values))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(stmt);
    if source.is_empty() {
        return bad("数据表为空，无法刷数");
    }
    fn calc(
        rule: &RowRule,
        fields: &[Field],
        values: &[Option<String>],
        depth: usize,
    ) -> Result<Decimal> {
        if depth > 64 {
            return bad("规则嵌套过深");
        }
        let get = |r| calc(r, fields, values, depth + 1);
        let overflow = || DeskError::InvalidInput("计算超出范围".into());
        Ok(match rule {
            RowRule::Field(name) => {
                let i = fields
                    .iter()
                    .position(|f| f.name == *name)
                    .ok_or_else(|| DeskError::InvalidInput(format!("字段不存在：{name}")))?;
                values[i]
                    .as_ref()
                    .ok_or_else(|| DeskError::InvalidInput(format!("字段 {name} 为空")))?
                    .parse()?
            }
            RowRule::Number(n) => n.parse()?,
            RowRule::Add(a, b) => Decimal(get(a)?.0.checked_add(get(b)?.0).ok_or_else(overflow)?),
            RowRule::Subtract(a, b) => {
                Decimal(get(a)?.0.checked_sub(get(b)?.0).ok_or_else(overflow)?)
            }
            RowRule::Multiply(a, b) => Decimal(rounded_div(
                get(a)?.0.checked_mul(get(b)?.0).ok_or_else(overflow)?,
                SCALE,
            )?),
            RowRule::Divide(a, b) => Decimal(rounded_div(
                get(a)?.0.checked_mul(SCALE).ok_or_else(overflow)?,
                get(b)?.0,
            )?),
        })
    }
    let mut updates = Vec::with_capacity(source.len());
    for (id, values) in source {
        let result = calc(rule, &plan.fields, &values, 0)
            .map_err(|e| DeskError::InvalidInput(format!("行 {id}：{e}")))?;
        let mut text = result.to_string();
        if matches!(target_field.kind, Kind::Integer) {
            text = text.trim_end_matches('0').trim_end_matches('.').to_string();
        }
        let mut field = target_field.clone();
        field.source = Source::Constant(text);
        updates.push((id, convert(None, &field, id as usize)?));
    }
    let mut stmt = tx.prepare(&format!(
        "UPDATE {} SET {}=?1 WHERE rowid=?2",
        quote(table)?,
        quote(target)?
    ))?;
    for (id, value) in &updates {
        if stmt.execute(rusqlite::params![value, id])? != 1 {
            return bad("数据行已变化，刷数已取消");
        }
    }
    drop(stmt);
    persist_field_rule(&tx, table, target, &FieldRule::Calculation(rule.clone()))?;
    tx.execute(
        "UPDATE report_workshop_meta SET value=value+1 WHERE key='data_revision'",
        [],
    )?;
    tx.commit()?;
    Ok(updates.len())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Template {
    pub title: String,
    pub group_table: String,
    pub group_field: String,
    pub group_filters: Vec<(String, String)>,
    pub columns: Vec<(String, Rule)>,
    #[serde(default)]
    pub cells: Vec<TemplateCell>,
    #[serde(default)]
    pub sort_column: Option<usize>,
    #[serde(default)]
    pub descending: bool,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default = "default_dynamic_row")]
    pub dynamic_row: u32,
    #[serde(default)]
    pub group_column: u16,
}
fn default_dynamic_row() -> u32 {
    2
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemplateCell {
    pub row: u32,
    pub column: u16,
    pub value: CellValue,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CellValue {
    Text(String),
    Calculated { group: String, rule: Rule },
}
#[derive(Clone, Debug)]
pub enum ResolvedCell {
    Text(String),
    Number(Decimal),
}

/// Persist immutable template versions. Database and UI integration remain pending.
pub fn save_template(db: &mut Connection, id: &str, template: &Template) -> Result<i64> {
    if id.trim().is_empty() || template.title.trim().is_empty() || template.columns.is_empty() {
        return bad("模板编号、标题及报表列不能为空");
    }
    validate_template(db, template)?;
    let content = serde_json::to_string(template)?;
    let tx = db.transaction()?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS report_workshop_templates (template_id TEXT NOT NULL, version INTEGER NOT NULL, content TEXT NOT NULL, PRIMARY KEY(template_id, version))")?;
    let version: i64 = tx.query_row(
        "SELECT COALESCE(MAX(version),0)+1 FROM report_workshop_templates WHERE template_id=?1",
        [id],
        |r| r.get(0),
    )?;
    tx.execute(
        "INSERT INTO report_workshop_templates(template_id,version,content) VALUES(?1,?2,?3)",
        rusqlite::params![id, version, content],
    )?;
    tx.commit()?;
    Ok(version)
}

pub fn record_export(
    db: &mut Connection,
    id: &str,
    version: i64,
    template: &Template,
    revision: i64,
    path: &Path,
) -> Result<()> {
    record_workbook_export(
        db,
        &[(id.to_string(), version, template.clone())],
        revision,
        path,
    )
}

pub fn record_workbook_export(
    db: &mut Connection,
    templates: &[(String, i64, Template)],
    revision: i64,
    path: &Path,
) -> Result<()> {
    if templates.is_empty() {
        return bad("工作簿没有模板");
    }
    let hash = file_hash(path)?;
    let tx = db.transaction()?;
    for (id, version, template) in templates {
        let saved: String = tx.query_row(
            "SELECT content FROM report_workshop_templates WHERE template_id=?1 AND version=?2",
            rusqlite::params![id, version],
            |r| r.get(0),
        )?;
        if saved != serde_json::to_string(template)? {
            return bad("导出模板与保存的版本不一致");
        }
    }
    let current: i64 = tx.query_row(
        "SELECT value FROM report_workshop_meta WHERE key='data_revision'",
        [],
        |r| r.get(0),
    )?;
    if current != revision {
        return Err(DeskError::ConcurrentModification);
    }
    tx.execute_batch("CREATE TABLE IF NOT EXISTS report_workshop_runs (id TEXT PRIMARY KEY,template_id TEXT NOT NULL,version INTEGER NOT NULL,data_revision INTEGER NOT NULL,output_path TEXT NOT NULL,sha256 TEXT NOT NULL,created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP)")?;
    for (id, version, _) in templates {
        tx.execute("INSERT INTO report_workshop_runs(id,template_id,version,data_revision,output_path,sha256) VALUES(?1,?2,?3,?4,?5,?6)",rusqlite::params![uuid::Uuid::new_v4().to_string(),id,version,revision,path.to_string_lossy().as_ref(),hash])?;
    }
    tx.commit()?;
    Ok(())
}

fn validate_template(db: &Connection, template: &Template) -> Result<()> {
    let mut fields = vec![template.group_field.as_str()];
    fields.extend(template.group_filters.iter().map(|(name, _)| name.as_str()));
    require_columns(db, &template.group_table, &fields)?;
    fn rule(db: &Connection, item: &Rule, depth: usize) -> Result<()> {
        if depth > 64 {
            return bad("规则嵌套过深");
        }
        match item {
            Rule::Number(s) => {
                s.parse::<Decimal>()?;
            }
            Rule::Sum {
                table,
                field,
                group_field,
                filters,
                all_rows,
            } => {
                let mut names = vec![field.as_str()];
                if !all_rows {
                    names.push(group_field.as_str());
                }
                names.extend(filters.iter().map(|(n, _)| n.as_str()));
                require_columns(db, table, &names)?;
            }
            Rule::LinkedSum {
                left_table,
                right_table,
                amount_field,
                group_field,
                keys,
                right_filters,
            } => {
                if keys.is_empty() {
                    return bad("关联取数至少需要一组关联字段");
                }
                let mut left = vec![amount_field.as_str(), group_field.as_str()];
                left.extend(keys.iter().map(|(l, _)| l.as_str()));
                let mut right = keys.iter().map(|(_, r)| r.as_str()).collect::<Vec<_>>();
                right.extend(right_filters.iter().map(|(f, _)| f.as_str()));
                require_columns(db, left_table, &left)?;
                require_columns(db, right_table, &right)?;
            }
            Rule::Add(a, b) | Rule::Subtract(a, b) | Rule::Multiply(a, b) | Rule::Divide(a, b) => {
                rule(db, a, depth + 1)?;
                rule(db, b, depth + 1)?;
            }
        }
        Ok(())
    }
    for (name, item) in &template.columns {
        if name.trim().is_empty() {
            return bad("报表列名不能为空");
        }
        rule(db, item, 0)?;
    }
    let mut positions = HashSet::new();
    if template.dynamic_row < 2
        || template.dynamic_row >= 1_048_576
        || usize::from(template.group_column) + template.columns.len() >= 16_384
    {
        return bad("动态行起始位置超出范围");
    }
    for cell in &template.cells {
        if cell.row >= 1_048_576
            || cell.column >= 16_384
            || !positions.insert((cell.row, cell.column))
        {
            return bad("模板单元格位置重复或超出 Excel 范围");
        }
        let in_columns = cell.column >= template.group_column
            && (cell.column as usize)
                <= usize::from(template.group_column) + template.columns.len();
        if cell.row == 0 && cell.column == 0
            || cell.row == template.dynamic_row - 1 && in_columns
            || cell.row == template.dynamic_row && in_columns
        {
            return bad("自定义单元格与标题、表头或动态行重叠");
        }
        if let CellValue::Calculated { rule: item, .. } = &cell.value {
            rule(db, item, 0)?;
        }
    }
    Ok(())
}

pub fn generate_cells(db: &Connection, template: &Template) -> Result<Vec<ResolvedCell>> {
    template
        .cells
        .iter()
        .map(|cell| match &cell.value {
            CellValue::Text(s) => Ok(ResolvedCell::Text(s.clone())),
            CellValue::Calculated { group, rule } => {
                Ok(ResolvedCell::Number(evaluate(db, rule, group)?))
            }
        })
        .collect()
}

pub fn generate_rows(db: &Connection, template: &Template) -> Result<Vec<(String, Vec<Decimal>)>> {
    if template.columns.is_empty() {
        return bad("请至少添加一个报表列");
    }
    validate_template(db, template)?;
    let mut required = vec![template.group_field.as_str()];
    required.extend(
        template
            .group_filters
            .iter()
            .map(|(field, _)| field.as_str()),
    );
    require_columns(db, &template.group_table, &required)?;
    let table = quote(&template.group_table)?;
    let field = quote(&template.group_field)?;
    let mut sql = format!("SELECT DISTINCT CAST({field} AS TEXT) FROM {table}");
    for (i, (name, _)) in template.group_filters.iter().enumerate() {
        sql.push_str(if i == 0 { " WHERE " } else { " AND " });
        sql.push_str(&format!("{}=?", quote(name)?));
    }
    sql.push_str(&format!(" ORDER BY {field}"));
    let mut stmt = db.prepare(&sql)?;
    let groups = stmt
        .query_map(
            params_from_iter(template.group_filters.iter().map(|(_, v)| v)),
            |row| row.get::<_, String>(0),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if groups.is_empty() {
        return bad("动态行没有匹配数据");
    }
    let mut result = groups
        .into_iter()
        .map(|group| {
            let values = template
                .columns
                .iter()
                .map(|(_, rule)| evaluate(db, rule, &group))
                .collect::<Result<Vec<_>>>()?;
            Ok((group, values))
        })
        .collect::<Result<Vec<_>>>()?;
    if let Some(index) = template.sort_column {
        if index >= template.columns.len() {
            return bad("排序列不存在");
        }
        result.sort_by(|a, b| {
            let ordering = a.1[index].0.cmp(&b.1[index].0);
            let ordering = if template.descending {
                ordering.reverse()
            } else {
                ordering
            };
            ordering.then_with(|| a.0.cmp(&b.0))
        });
    }
    if let Some(limit) = template.limit {
        if limit == 0 {
            return bad("行数上限必须大于零");
        }
        result.truncate(limit);
    }
    Ok(result)
}

/// Writes a new output only after the entire workbook has been built successfully.
pub fn export_report(
    path: &Path,
    template: &Template,
    rows: &[(String, Vec<Decimal>)],
    cells: &[ResolvedCell],
) -> Result<()> {
    export_workbook(path, &[(template, rows, cells)])
}

pub fn export_workbook(path: &Path, reports: &[SheetExport<'_>]) -> Result<()> {
    if path.exists() {
        return bad("输出文件已存在，请选择新文件名");
    }
    if reports.is_empty() {
        return bad("请选择要导出的工作表");
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| DeskError::InvalidPath("请选择输出目录".into()))?;
    let mut workbook = rust_xlsxwriter::Workbook::new();
    let error = |e| DeskError::InvalidInput(format!("写入报表失败: {e}"));
    for (template, rows, cells) in reports {
        let sheet = workbook.add_worksheet();
        sheet.set_name(&template.title).map_err(error)?;
        write_report_sheet(sheet, template, rows, cells)?;
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    workbook
        .save_to_writer(temporary.as_file_mut())
        .map_err(error)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(path)
        .map_err(|e| DeskError::Io(e.error))?;
    Ok(())
}

fn write_report_sheet(
    sheet: &mut rust_xlsxwriter::Worksheet,
    template: &Template,
    rows: &[(String, Vec<Decimal>)],
    cells: &[ResolvedCell],
) -> Result<()> {
    if rows.len() + template.dynamic_row as usize > 1_048_576
        || template.columns.len() + template.group_column as usize >= 16_384
        || cells.len() != template.cells.len()
    {
        return bad("结果超出 Excel 工作表限制");
    }
    let error = |e| DeskError::InvalidInput(format!("写入报表失败: {e}"));
    sheet.write_string(0, 0, &template.title).map_err(error)?;
    sheet
        .write_string(
            template.dynamic_row - 1,
            template.group_column,
            &template.group_field,
        )
        .map_err(error)?;
    for (i, (name, _)) in template.columns.iter().enumerate() {
        sheet
            .write_string(
                template.dynamic_row - 1,
                template.group_column + (i + 1) as u16,
                name,
            )
            .map_err(error)?;
    }
    for (i, (group, values)) in rows.iter().enumerate() {
        if values.len() != template.columns.len() {
            return bad("结果列数与模板不一致");
        }
        sheet
            .write_string(
                template.dynamic_row + i as u32,
                template.group_column,
                group,
            )
            .map_err(error)?;
        for (j, value) in values.iter().enumerate() {
            let text = value.to_string();
            let compact = text
                .trim_start_matches('-')
                .trim_end_matches('0')
                .trim_end_matches('.')
                .replace('.', "");
            let digits = compact.trim_start_matches('0').len();
            if digits > 15 {
                return bad("结果超过 Excel 数值精度，请缩小数据范围");
            }
            let number: f64 = text
                .parse()
                .map_err(|_| DeskError::InvalidInput("结果无法写入 Excel".into()))?;
            sheet
                .write_number(
                    template.dynamic_row + i as u32,
                    template.group_column + (j + 1) as u16,
                    number,
                )
                .map_err(error)?;
        }
    }
    for (cell, value) in template.cells.iter().zip(cells) {
        let row = if cell.row > template.dynamic_row {
            cell.row
                .checked_add(rows.len().saturating_sub(1) as u32)
                .ok_or_else(|| DeskError::InvalidInput("单元格超出工作表范围".into()))?
        } else {
            cell.row
        };
        if row >= 1_048_576 {
            return bad("动态行展开后单元格超出工作表范围");
        }
        match value {
            ResolvedCell::Text(s) => {
                sheet.write_string(row, cell.column, s).map_err(error)?;
            }
            ResolvedCell::Number(value) => {
                let text = value.to_string();
                let compact = text
                    .trim_start_matches('-')
                    .trim_end_matches('0')
                    .trim_end_matches('.')
                    .replace('.', "");
                if compact.trim_start_matches('0').len() > 15 {
                    return bad("结果超过 Excel 数值精度");
                }
                sheet
                    .write_number(
                        row,
                        cell.column,
                        text.parse::<f64>()
                            .map_err(|_| DeskError::InvalidInput("结果无法写入 Excel".into()))?,
                    )
                    .map_err(error)?;
            }
        }
    }
    Ok(())
}

/// Fixed precision (eight decimal places), with checked arithmetic and explicit rounding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decimal(i128);
const SCALE: i128 = 100_000_000;
impl FromStr for Decimal {
    type Err = DeskError;
    fn from_str(input: &str) -> Result<Self> {
        let s = input.trim();
        let (negative, rest) = if let Some(x) = s.strip_prefix('-') {
            (true, x)
        } else if let Some(x) = s.strip_prefix('+') {
            (false, x)
        } else {
            (false, s)
        };
        let mut parts = rest.split('.');
        let whole = parts.next().unwrap_or("");
        let frac = parts.next().unwrap_or("");
        if parts.next().is_some()
            || whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || !frac.bytes().all(|b| b.is_ascii_digit())
            || frac.len() > 8
        {
            return bad(format!("无效数字或小数超过 8 位: {input}"));
        }
        let whole: i128 = whole
            .parse()
            .map_err(|_| DeskError::InvalidInput("数字超出范围".into()))?;
        let fraction: i128 = if frac.is_empty() {
            0
        } else {
            frac.parse::<i128>()
                .map_err(|_| DeskError::InvalidInput("数字超出范围".into()))?
                * 10_i128.pow(8 - frac.len() as u32)
        };
        let value = whole
            .checked_mul(SCALE)
            .and_then(|v| v.checked_add(fraction))
            .ok_or_else(|| DeskError::InvalidInput("数字超出范围".into()))?;
        Ok(Self(if negative { -value } else { value }))
    }
}
impl std::fmt::Display for Decimal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = self.0.unsigned_abs();
        write!(
            f,
            "{}{}.{:08}",
            if self.0 < 0 { "-" } else { "" },
            n / SCALE as u128,
            n % SCALE as u128
        )
    }
}
fn rounded_div(a: i128, b: i128) -> Result<i128> {
    if b == 0 {
        return bad("除数不能为零");
    }
    let q = a
        .checked_div(b)
        .ok_or_else(|| DeskError::InvalidInput("计算超出范围".into()))?;
    let rem = a
        .checked_rem(b)
        .ok_or_else(|| DeskError::InvalidInput("计算超出范围".into()))?;
    if rem.unsigned_abs().saturating_mul(2) >= b.unsigned_abs() {
        q.checked_add(if (a < 0) == (b < 0) { 1 } else { -1 })
            .ok_or_else(|| DeskError::InvalidInput("计算超出范围".into()))
    } else {
        Ok(q)
    }
}

/// Computes one cell for a group. Missing values and zero division are errors.
pub fn evaluate(db: &Connection, rule: &Rule, group: &str) -> Result<Decimal> {
    fn inner(db: &Connection, rule: &Rule, group: &str, depth: usize) -> Result<Decimal> {
        if depth > 64 {
            return bad("规则嵌套过深");
        }
        let child = |r| inner(db, r, group, depth + 1);
        let overflow = || DeskError::InvalidInput("计算超出范围".into());
        let value = match rule {
            Rule::Number(value) => value.parse()?,
            Rule::Sum {
                table,
                field,
                group_field,
                filters,
                all_rows,
            } => {
                let mut required = vec![field.as_str()];
                if !all_rows {
                    required.push(group_field.as_str());
                }
                required.extend(filters.iter().map(|(name, _)| name.as_str()));
                require_columns(db, table, &required)?;
                let table = quote(table)?;
                let field = quote(field)?;
                let mut sql = format!("SELECT CAST({field} AS TEXT) FROM {table}");
                let mut conditions = Vec::new();
                if !all_rows {
                    conditions.push(format!("{}=?", quote(group_field)?));
                }
                for (name, _) in filters {
                    conditions.push(format!("{}=?", quote(name)?));
                }
                if !conditions.is_empty() {
                    sql.push_str(" WHERE ");
                    sql.push_str(&conditions.join(" AND "));
                }
                let mut stmt = db.prepare(&sql)?;
                let mut args = Vec::new();
                if !all_rows {
                    args.push(group);
                }
                args.extend(filters.iter().map(|(_, value)| value.as_str()));
                let mut rows = stmt.query(params_from_iter(args))?;
                let mut total = 0_i128;
                let mut count = 0;
                while let Some(row) = rows.next()? {
                    let raw: Option<String> = row.get(0)?;
                    let value: Decimal = raw
                        .ok_or_else(|| DeskError::InvalidInput("取数存在空值".into()))?
                        .parse()?;
                    total = total.checked_add(value.0).ok_or_else(overflow)?;
                    count += 1;
                }
                if count == 0 {
                    return bad("取数没有匹配数据");
                }
                Decimal(total)
            }
            Rule::LinkedSum {
                left_table,
                right_table,
                amount_field,
                group_field,
                keys,
                right_filters,
            } => {
                if keys.is_empty() {
                    return bad("关联取数至少需要一组关联字段");
                }
                let mut left = vec![amount_field.as_str(), group_field.as_str()];
                left.extend(keys.iter().map(|(l, _)| l.as_str()));
                let mut right = keys.iter().map(|(_, r)| r.as_str()).collect::<Vec<_>>();
                right.extend(right_filters.iter().map(|(f, _)| f.as_str()));
                require_columns(db, left_table, &left)?;
                require_columns(db, right_table, &right)?;
                let mut conditions = keys
                    .iter()
                    .map(|(l, r)| Ok(format!("l.{}=r.{}", quote(l)?, quote(r)?)))
                    .collect::<Result<Vec<_>>>()?;
                for (name, _) in right_filters {
                    conditions.push(format!("r.{}=?", quote(name)?));
                }
                let sql = format!(
                    "SELECT CAST(l.{} AS TEXT) FROM {} l WHERE l.{}=? AND EXISTS (SELECT 1 FROM {} r WHERE {})",
                    quote(amount_field)?,
                    quote(left_table)?,
                    quote(group_field)?,
                    quote(right_table)?,
                    conditions.join(" AND ")
                );
                let mut stmt = db.prepare(&sql)?;
                let args =
                    std::iter::once(group).chain(right_filters.iter().map(|(_, v)| v.as_str()));
                let mut rows = stmt.query(params_from_iter(args))?;
                let mut total = 0_i128;
                let mut count = 0;
                while let Some(row) = rows.next()? {
                    let raw: Option<String> = row.get(0)?;
                    let value: Decimal = raw
                        .ok_or_else(|| DeskError::InvalidInput("关联取数存在空值".into()))?
                        .parse()?;
                    total = total.checked_add(value.0).ok_or_else(overflow)?;
                    count += 1;
                }
                if count == 0 {
                    return bad("关联取数没有匹配数据");
                }
                Decimal(total)
            }
            Rule::Add(a, b) => Decimal(child(a)?.0.checked_add(child(b)?.0).ok_or_else(overflow)?),
            Rule::Subtract(a, b) => {
                Decimal(child(a)?.0.checked_sub(child(b)?.0).ok_or_else(overflow)?)
            }
            Rule::Multiply(a, b) => Decimal(rounded_div(
                child(a)?.0.checked_mul(child(b)?.0).ok_or_else(overflow)?,
                SCALE,
            )?),
            Rule::Divide(a, b) => Decimal(rounded_div(
                child(a)?.0.checked_mul(SCALE).ok_or_else(overflow)?,
                child(b)?.0,
            )?),
        };
        Ok(value)
    }
    inner(db, rule, group, 0)
}

fn bad<T>(message: impl Into<String>) -> Result<T> {
    Err(DeskError::InvalidInput(message.into()))
}

fn quote(name: &str) -> Result<String> {
    if name != name.trim() || name.is_empty() || name.contains('\0') || name.len() > 128 {
        return bad("表名或字段名不能为空、包含首尾空格或空字符，也不能超过 128 字节");
    }
    Ok(format!("\"{}\"", name.replace('"', "\"\"")))
}

fn require_columns(db: &Connection, table: &str, columns: &[&str]) -> Result<()> {
    if !db.table_exists(None, "report_workshop_tables")? {
        return bad("还没有创建报表数据表");
    }
    let managed: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM report_workshop_tables WHERE name=?1)",
        [table],
        |r| r.get(0),
    )?;
    if !managed {
        return bad(format!("数据表不属于报表工坊: {table}"));
    }
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )?;
    if !exists {
        return bad(format!("数据表不存在: {table}"));
    }
    let mut stmt = db.prepare(&format!("PRAGMA table_info({})", quote(table)?))?;
    let names = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for col in columns {
        if !names.iter().any(|name| name == col) {
            return bad(format!("表 {table} 中没有字段 {col}"));
        }
    }
    Ok(())
}

/// Adds an optional column without rewriting existing rows. The mapping is persisted.
pub fn add_field(db: &mut Connection, table: &str, field: Field) -> Result<()> {
    if field.required {
        return bad("现有数据表新增字段必须允许为空");
    }
    quote(&field.name)?;
    if ["rowid", "_rowid_", "oid"]
        .iter()
        .any(|n| field.name.eq_ignore_ascii_case(n))
    {
        return bad("字段名不能使用数据库保留行号");
    }
    require_columns(db, table, &[])?;
    let tx = db.transaction()?;
    let raw: String = tx.query_row(
        "SELECT plan FROM report_workshop_tables WHERE name=?1",
        [table],
        |r| r.get(0),
    )?;
    let mut plan: SheetImport = serde_json::from_str(&raw)?;
    if plan
        .fields
        .iter()
        .any(|f| f.name.eq_ignore_ascii_case(&field.name))
    {
        return bad("字段名已存在");
    }
    let kind = match field.kind {
        Kind::Text | Kind::Decimal => "TEXT",
        Kind::Integer => "INTEGER",
        Kind::Real => "REAL",
    };
    tx.execute_batch(&format!(
        "ALTER TABLE {} ADD COLUMN {} {kind}",
        quote(table)?,
        quote(&field.name)?
    ))?;
    plan.fields.push(field);
    tx.execute(
        "UPDATE report_workshop_tables SET plan=?1 WHERE name=?2",
        rusqlite::params![serde_json::to_string(&plan)?, table],
    )?;
    tx.execute(
        "UPDATE report_workshop_meta SET value=value+1 WHERE key='data_revision'",
        [],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn rename_field(db: &mut Connection, table: &str, old: &str, new: &str) -> Result<()> {
    quote(new)?;
    if ["rowid", "_rowid_", "oid"]
        .iter()
        .any(|n| new.eq_ignore_ascii_case(n))
    {
        return bad("字段名不能使用数据库保留行号");
    }
    require_columns(db, table, &[old])?;
    fn rule_uses(rule: &Rule, table: &str, field: &str) -> bool {
        match rule {
            Rule::Sum {
                table: t,
                field: f,
                group_field,
                filters,
                ..
            } => {
                t == table
                    && (f == field
                        || group_field == field
                        || filters.iter().any(|(name, _)| name == field))
            }
            Rule::Add(a, b) | Rule::Subtract(a, b) | Rule::Multiply(a, b) | Rule::Divide(a, b) => {
                rule_uses(a, table, field) || rule_uses(b, table, field)
            }
            Rule::LinkedSum {
                left_table,
                right_table,
                amount_field,
                group_field,
                keys,
                right_filters,
            } => {
                (left_table == table
                    && (amount_field == field
                        || group_field == field
                        || keys.iter().any(|(l, _)| l == field)))
                    || (right_table == table
                        && (keys.iter().any(|(_, r)| r == field)
                            || right_filters.iter().any(|(f, _)| f == field)))
            }
            Rule::Number(_) => false,
        }
    }
    let tx = db.transaction()?;
    if tx.table_exists(None, "report_workshop_field_rules")? {
        fn uses(rule: &RowRule, name: &str) -> bool {
            match rule {
                RowRule::Field(f) => f == name,
                RowRule::Number(_) => false,
                RowRule::Add(a, b)
                | RowRule::Subtract(a, b)
                | RowRule::Multiply(a, b)
                | RowRule::Divide(a, b) => uses(a, name) || uses(b, name),
            }
        }
        let mut stmt = tx.prepare(
            "SELECT field_name,rule FROM report_workshop_field_rules WHERE table_name=?1",
        )?;
        let rules = stmt
            .query_map([table], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        for (target, json) in rules {
            let rule: FieldRule = serde_json::from_str(&json)?;
            if target == old || matches!(&rule,FieldRule::Calculation(expr) if uses(expr,old)) {
                return bad(format!("字段被 {target} 的刷数规则引用，不能直接重命名"));
            }
        }
    }
    if tx.table_exists(None, "report_workshop_templates")? {
        let mut stmt =
            tx.prepare("SELECT template_id,version,content FROM report_workshop_templates")?;
        let templates = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        for (id, version, raw) in templates {
            let template: Template = serde_json::from_str(&raw)?;
            let used=template.group_table==table && (template.group_field==old || template.group_filters.iter().any(|(f,_)|f==old)) || template.columns.iter().any(|(_,r)|rule_uses(r,table,old)) || template.cells.iter().any(|c|matches!(&c.value,CellValue::Calculated{rule,..} if rule_uses(rule,table,old)));
            if used {
                return bad(format!(
                    "字段被模板 {id} 第 {version} 版引用，无法直接重命名"
                ));
            }
        }
    }
    let raw: String = tx.query_row(
        "SELECT plan FROM report_workshop_tables WHERE name=?1",
        [table],
        |r| r.get(0),
    )?;
    let mut plan: SheetImport = serde_json::from_str(&raw)?;
    if plan.fields.iter().any(|f| f.name.eq_ignore_ascii_case(new)) {
        return bad("字段名已存在");
    }
    let field = plan
        .fields
        .iter_mut()
        .find(|f| f.name == old)
        .ok_or_else(|| DeskError::InvalidInput("字段不存在".into()))?;
    field.name = new.into();
    tx.execute_batch(&format!(
        "ALTER TABLE {} RENAME COLUMN {} TO {}",
        quote(table)?,
        quote(old)?,
        quote(new)?
    ))?;
    tx.execute(
        "UPDATE report_workshop_tables SET plan=?1 WHERE name=?2",
        rusqlite::params![serde_json::to_string(&plan)?, table],
    )?;
    tx.execute(
        "UPDATE report_workshop_meta SET value=value+1 WHERE key='data_revision'",
        [],
    )?;
    tx.commit()?;
    Ok(())
}

/// Read a bounded page using the stable SQLite row id. Values are rendered as text.
pub fn read_rows(db: &Connection, table: &str, offset: usize, limit: usize) -> Result<DataPage> {
    require_columns(db, table, &[])?;
    if limit == 0 || limit > 200 {
        return bad("每页数据条数应在 1 到 200 之间");
    }
    let raw: String = db.query_row(
        "SELECT plan FROM report_workshop_tables WHERE name=?1",
        [table],
        |r| r.get(0),
    )?;
    let plan: SheetImport = serde_json::from_str(&raw)?;
    let names = plan
        .fields
        .iter()
        .map(|f| quote(&f.name).map(|name| format!("CAST({name} AS TEXT)")))
        .collect::<Result<Vec<_>>>()?
        .join(",");
    let mut stmt = db.prepare(&format!(
        "SELECT rowid,{names} FROM {} ORDER BY rowid LIMIT ?1 OFFSET ?2",
        quote(table)?
    ))?;
    let data = stmt
        .query_map(rusqlite::params![limit as i64, offset as i64], |r| {
            let mut values = Vec::new();
            for i in 0..plan.fields.len() {
                values.push(r.get(i + 1)?);
            }
            Ok((r.get(0)?, values))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let revision = db.query_row(
        "SELECT value FROM report_workshop_meta WHERE key='data_revision'",
        [],
        |r| r.get(0),
    )?;
    Ok((revision, data))
}

/// Updates one cell only if the page has not become stale.
pub fn update_cell(
    db: &mut Connection,
    table: &str,
    rowid: i64,
    field_name: &str,
    text: &str,
    expected_revision: i64,
) -> Result<()> {
    require_columns(db, table, &[field_name])?;
    let tx = db.transaction()?;
    let revision: i64 = tx.query_row(
        "SELECT value FROM report_workshop_meta WHERE key='data_revision'",
        [],
        |r| r.get(0),
    )?;
    if revision != expected_revision {
        return Err(DeskError::ConcurrentModification);
    }
    let raw: String = tx.query_row(
        "SELECT plan FROM report_workshop_tables WHERE name=?1",
        [table],
        |r| r.get(0),
    )?;
    let plan: SheetImport = serde_json::from_str(&raw)?;
    let mut field = plan
        .fields
        .into_iter()
        .find(|f| f.name == field_name)
        .ok_or_else(|| DeskError::InvalidInput("字段不存在".into()))?;
    field.source = Source::Constant(text.into());
    let value = convert(None, &field, 0)?;
    let count = tx.execute(
        &format!(
            "UPDATE {} SET {}=?1 WHERE rowid=?2",
            quote(table)?,
            quote(field_name)?
        ),
        rusqlite::params![value, rowid],
    )?;
    if count != 1 {
        return bad("数据行已不存在");
    }
    tx.execute(
        "UPDATE report_workshop_meta SET value=value+1 WHERE key='data_revision'",
        [],
    )?;
    tx.commit()?;
    Ok(())
}

fn convert(data: Option<&Data>, field: &Field, row: usize) -> Result<Value> {
    let text = match (&field.source, data) {
        (Source::Constant(value), _) if value.trim().is_empty() => None,
        (Source::Constant(value), _) => Some(value.clone()),
        (_, None | Some(Data::Empty)) => None,
        (_, Some(Data::String(value))) if value.trim().is_empty() => None,
        (_, Some(Data::Error(error))) => {
            return bad(format!(
                "第 {row} 行字段 {} 包含 Excel 错误: {error:?}",
                field.name
            ));
        }
        (_, Some(value)) => Some(value.to_string()),
    };
    let Some(text) = text else {
        return if field.required {
            bad(format!("第 {row} 行字段 {} 不能为空", field.name))
        } else {
            Ok(Value::Null)
        };
    };
    let invalid =
        || DeskError::InvalidInput(format!("第 {row} 行字段 {} 的值不符合字段类型", field.name));
    Ok(match field.kind {
        Kind::Text => Value::Text(text),
        Kind::Integer => Value::Integer(text.trim().parse().map_err(|_| invalid())?),
        Kind::Real => {
            let n: f64 = text.trim().parse().map_err(|_| invalid())?;
            if !n.is_finite() {
                return Err(invalid());
            }
            Value::Real(n)
        }
        Kind::Decimal => Value::Text(text.parse::<Decimal>()?.to_string()),
    })
}

/// Creates new tables and imports all requested sheets as one transaction.
/// Existing tables are never overwritten. All worksheet ranges are inspected
/// before opening the transaction, so a missing sheet cannot produce a partial import.
#[cfg(test)]
pub fn import_new_tables(
    db: &mut Connection,
    path: &Path,
    plans: &[SheetImport],
) -> Result<Vec<usize>> {
    import_tables(db, path, plans, ImportMode::Create)
}

#[cfg(test)]
pub fn import_tables(
    db: &mut Connection,
    path: &Path,
    plans: &[SheetImport],
    mode: ImportMode,
) -> Result<Vec<usize>> {
    import_tables_checked(db, path, plans, mode, None)
}

pub fn import_tables_checked(
    db: &mut Connection,
    path: &Path,
    plans: &[SheetImport],
    mode: ImportMode,
    expected_hash: Option<&str>,
) -> Result<Vec<usize>> {
    if let Some(expected) = expected_hash
        && file_hash(path)? != expected
    {
        return Err(DeskError::ConcurrentModification);
    }
    if plans.is_empty() {
        return bad("请至少配置一个工作表");
    }
    let mut names = HashSet::new();
    let mut book = open_workbook_auto(path)
        .map_err(|e| DeskError::InvalidInput(format!("无法打开工作簿: {e}")))?;
    let text_cells = xlsx_text::read(
        path,
        &plans.iter().map(|p| p.sheet.clone()).collect::<Vec<_>>(),
    )?;
    let mut ranges = Vec::with_capacity(plans.len());
    for plan in plans {
        quote(&plan.table)?;
        if plan
            .table
            .trim()
            .to_ascii_lowercase()
            .starts_with("report_workshop_")
            || plan
                .table
                .trim()
                .to_ascii_lowercase()
                .starts_with("sqlite_")
            || !names.insert(plan.table.trim().to_lowercase())
        {
            return bad("表名重复或使用了 SQLite 保留名称");
        }
        if plan.fields.is_empty() || plan.first_data_row == 0 {
            return bad("字段不能为空，数据起始行必须从 1 开始");
        }
        let mut fields = HashSet::new();
        for field in &plan.fields {
            quote(&field.name)?;
            if ["rowid", "_rowid_", "oid"]
                .iter()
                .any(|n| field.name.eq_ignore_ascii_case(n))
            {
                return bad("字段名不能使用数据库保留行号");
            }
            if !fields.insert(field.name.trim().to_lowercase()) {
                return bad("同一张表存在重复字段名");
            }
        }
        let mut range = book
            .worksheet_range(&plan.sheet)
            .map_err(|e| DeskError::InvalidInput(format!("读取工作表 {} 失败: {e}", plan.sheet)))?;
        if let Some(cells) = text_cells.get(&plan.sheet) {
            xlsx_text::apply(&mut range, cells);
        }
        ranges.push(range);
    }
    let tx = db.transaction()?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS report_workshop_tables(name TEXT PRIMARY KEY COLLATE NOCASE, plan TEXT NOT NULL)")?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS report_workshop_meta(key TEXT PRIMARY KEY,value INTEGER NOT NULL); INSERT OR IGNORE INTO report_workshop_meta(key,value) VALUES('data_revision',0)")?;
    let mut totals = Vec::new();
    for (plan, range) in plans.iter().zip(ranges) {
        let table = quote(&plan.table)?;
        let definitions = plan
            .fields
            .iter()
            .map(|field| {
                let sql_type = match field.kind {
                    Kind::Text => "TEXT",
                    Kind::Integer => "INTEGER",
                    Kind::Real => "REAL",
                    Kind::Decimal => "TEXT",
                };
                Ok(format!(
                    "{} {}{}",
                    quote(&field.name)?,
                    sql_type,
                    if field.required { " NOT NULL" } else { "" }
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        if mode == ImportMode::Create {
            tx.execute_batch(&format!("CREATE TABLE {table} ({})", definitions.join(",")))?;
            tx.execute(
                "INSERT INTO report_workshop_tables(name,plan) VALUES(?1,?2)",
                rusqlite::params![plan.table.trim(), serde_json::to_string(plan)?],
            )?;
        } else {
            let saved: String = tx
                .query_row(
                    "SELECT plan FROM report_workshop_tables WHERE name=?1",
                    [plan.table.trim()],
                    |r| r.get(0),
                )
                .map_err(|_| {
                    DeskError::InvalidInput(format!("无法找到已管理的数据表 {}", plan.table))
                })?;
            let previous: SheetImport = serde_json::from_str(&saved)?;
            if previous.fields.len() != plan.fields.len()
                || previous.fields.iter().zip(&plan.fields).any(|(a, b)| {
                    a.name != b.name
                        || std::mem::discriminant(&a.kind) != std::mem::discriminant(&b.kind)
                        || a.required != b.required
                })
            {
                return bad(format!("表 {} 字段配置与已有结构不一致", plan.table));
            }
            require_columns(
                &tx,
                &plan.table,
                &plan
                    .fields
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>(),
            )?;
            if mode == ImportMode::ReplaceAll {
                tx.execute(&format!("DELETE FROM {table}"), [])?;
            }
        }
        let placeholders = (0..plan.fields.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let column_names = plan
            .fields
            .iter()
            .map(|field| quote(&field.name))
            .collect::<Result<Vec<_>>>()?
            .join(",");
        let mut stmt = tx.prepare(&format!(
            "INSERT INTO {table} ({column_names}) VALUES ({placeholders})"
        ))?;
        let (start_row, start_col) = range.start().unwrap_or((0, 0));
        let mut count = 0;
        for (offset, row) in range.rows().enumerate() {
            let row_number = start_row as usize + offset + 1;
            if row_number < plan.first_data_row
                || row.iter().all(|v| {
                    matches!(v, Data::Empty) || matches!(v, Data::String(s) if s.trim().is_empty())
                })
            {
                continue;
            }
            if !plan.end_marker.is_empty()
                && start_col == 0
                && row
                    .first()
                    .is_some_and(|cell| cell.to_string().trim() == plan.end_marker)
            {
                break;
            }
            if let Some(col) = plan.skip_empty_column {
                let value = col.checked_sub(start_col as usize).and_then(|i| row.get(i));
                if value.is_none()
                    || value.is_some_and(|v| {
                        matches!(v, Data::Empty)
                            || matches!(v,Data::String(s) if s.trim().is_empty())
                    })
                {
                    continue;
                }
            }
            let values = plan
                .fields
                .iter()
                .map(|field| {
                    let data = match field.source {
                        Source::Column(col) => {
                            col.checked_sub(start_col as usize).and_then(|i| row.get(i))
                        }
                        Source::Constant(_) => None,
                    };
                    convert(data, field, row_number)
                })
                .collect::<Result<Vec<_>>>()?;
            stmt.execute(params_from_iter(values))?;
            count += 1;
        }
        drop(stmt);
        if count == 0 {
            return bad(format!(
                "工作表 {} 没有导入任何数据，整批操作已取消",
                plan.sheet
            ));
        }
        totals.push(count);
    }
    if let Some(expected) = expected_hash
        && file_hash(path)? != expected
    {
        return Err(DeskError::ConcurrentModification);
    }
    tx.execute(
        "UPDATE report_workshop_meta SET value=value+1 WHERE key='data_revision'",
        [],
    )?;
    tx.commit()?;
    Ok(totals)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_xlsxwriter::Workbook;

    fn field(name: &str, kind: Kind, source: Source) -> Field {
        Field {
            name: name.into(),
            kind,
            source,
            required: true,
        }
    }
    #[test]
    fn two_sheets_map_to_two_tables_and_failure_rolls_back_all() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.xlsx");
        let mut book = Workbook::new();
        let sheet = book.add_worksheet();
        sheet.set_name("欠费").unwrap();
        sheet.write_string(0, 1, "编号").unwrap();
        sheet.write_string(1, 1, "001").unwrap();
        sheet.write_number(1, 2, 120.5).unwrap();
        let sheet = book.add_worksheet();
        sheet.set_name("回款").unwrap();
        sheet.write_string(0, 0, "编号").unwrap();
        sheet.write_string(1, 0, "001").unwrap();
        sheet.write_number(1, 1, 20.0).unwrap();
        book.save(&path).unwrap();
        let plans = vec![
            SheetImport {
                sheet: "欠费".into(),
                table: "欠费表".into(),
                first_data_row: 2,
                end_marker: String::new(),
                skip_empty_column: None,
                fields: vec![
                    field("编号", Kind::Text, Source::Column(1)),
                    field("金额", Kind::Real, Source::Column(2)),
                    field("账期", Kind::Text, Source::Constant("202609".into())),
                ],
            },
            SheetImport {
                sheet: "回款".into(),
                table: "回款表".into(),
                first_data_row: 2,
                end_marker: String::new(),
                skip_empty_column: None,
                fields: vec![
                    field("编号", Kind::Text, Source::Column(0)),
                    field("金额", Kind::Real, Source::Column(1)),
                ],
            },
        ];
        let mut db = Connection::open_in_memory().unwrap();
        assert_eq!(
            import_new_tables(&mut db, &path, &plans).unwrap(),
            vec![1, 1]
        );
        assert!(
            import_tables_checked(
                &mut db,
                &path,
                &plans,
                ImportMode::ReplaceAll,
                Some("wrong-hash")
            )
            .is_err()
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM 欠费表", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        let row: (String, String, f64) = db
            .query_row("SELECT 编号,账期,金额 FROM 欠费表", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!(row, ("001".into(), "202609".into(), 120.5));
        let mut second = plans.clone();
        second[0].table = "临时表".into();
        assert!(import_new_tables(&mut db, &path, &second).is_err());
        assert!(!db.table_exists(None, "临时表").unwrap());
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM 欠费表", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        let sum = |table: &str| Rule::Sum {
            table: table.into(),
            field: "金额".into(),
            group_field: "编号".into(),
            filters: Vec::new(),
            all_rows: false,
        };
        let rule = Rule::Divide(
            Box::new(Rule::Multiply(
                Box::new(Rule::Subtract(
                    Box::new(Rule::Add(
                        Box::new(sum("欠费表")),
                        Box::new(Rule::Number("19.5".into())),
                    )),
                    Box::new(sum("回款表")),
                )),
                Box::new(Rule::Number("2".into())),
            )),
            Box::new(Rule::Number("4".into())),
        );
        assert_eq!(evaluate(&db, &rule, "001").unwrap(), "60".parse().unwrap());
        assert!(evaluate(&db, &rule, "不存在").is_err());
        assert!(
            evaluate(
                &db,
                &Rule::Divide(Box::new(rule), Box::new(Rule::Number("0".into()))),
                "001"
            )
            .is_err()
        );
        let template = Template {
            title: "欠费报表".into(),
            group_table: "欠费表".into(),
            group_field: "编号".into(),
            group_filters: Vec::new(),
            sort_column: None,
            descending: false,
            limit: None,
            dynamic_row: 2,
            group_column: 0,
            columns: vec![
                ("欠费".into(), sum("欠费表")),
                ("回款".into(), sum("回款表")),
            ],
            cells: Vec::new(),
        };
        assert_eq!(
            generate_rows(&db, &template).unwrap(),
            vec![(
                "001".into(),
                vec!["120.5".parse().unwrap(), "20".parse().unwrap()]
            )]
        );
        assert_eq!(save_template(&mut db, "sample", &template).unwrap(), 1);
        assert_eq!(save_template(&mut db, "sample", &template).unwrap(), 2);
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM report_workshop_templates WHERE template_id='sample'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
        assert_eq!(
            import_tables(&mut db, &path, &plans, ImportMode::Append).unwrap(),
            vec![1, 1]
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM 欠费表", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
        let mut invalid = plans.clone();
        invalid[1].fields[1].source = Source::Constant("not-a-number".into());
        assert!(import_tables(&mut db, &path, &invalid, ImportMode::ReplaceAll).is_err());
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM 欠费表", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            import_tables(&mut db, &path, &plans, ImportMode::ReplaceAll).unwrap(),
            vec![1, 1]
        );
        add_field(
            &mut db,
            "欠费表",
            Field {
                name: "地市编码".into(),
                kind: Kind::Text,
                source: Source::Constant(String::new()),
                required: false,
            },
        )
        .unwrap();
        let (revision, rows) = read_rows(&db, "欠费表", 0, 100).unwrap();
        assert_eq!(rows.len(), 1);
        update_cell(&mut db, "欠费表", rows[0].0, "地市编码", "0431", revision).unwrap();
        assert!(update_cell(&mut db, "欠费表", rows[0].0, "地市编码", "0432", revision).is_err());
        assert_eq!(
            db.query_row("SELECT 地市编码 FROM 欠费表", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "0431"
        );
        assert!(rename_field(&mut db, "欠费表", "金额", "债务").is_err());
        rename_field(&mut db, "欠费表", "地市编码", "地区代码").unwrap();
        assert_eq!(
            db.query_row("SELECT 地区代码 FROM 欠费表", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "0431"
        );
        let (revision, _) = read_rows(&db, "欠费表", 0, 100).unwrap();
        assert_eq!(
            fill_field(&mut db, "欠费表", "地区代码", "0007", revision).unwrap(),
            1
        );
        assert!(fill_field(&mut db, "欠费表", "地区代码", "bad", revision).is_err());
        assert_eq!(
            db.query_row("SELECT 地区代码 FROM 欠费表", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "0007"
        );
        add_field(
            &mut db,
            "欠费表",
            Field {
                name: "计算值".into(),
                kind: Kind::Decimal,
                source: Source::Constant(String::new()),
                required: false,
            },
        )
        .unwrap();
        let (revision, _) = read_rows(&db, "欠费表", 0, 100).unwrap();
        let rule = RowRule::Multiply(
            Box::new(RowRule::Field("金额".into())),
            Box::new(RowRule::Number("2".into())),
        );
        save_field_rule(
            &mut db,
            "欠费表",
            "计算值",
            &FieldRule::Calculation(rule.clone()),
        )
        .unwrap();
        assert_eq!(
            refresh_field(&mut db, "欠费表", "计算值", &rule, revision).unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT 计算值 FROM 欠费表", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "241.00000000"
        );
        assert!(refresh_field(&mut db, "欠费表", "计算值", &rule, revision).is_err());
        let saved:String=db.query_row("SELECT rule FROM report_workshop_field_rules WHERE table_name='欠费表' AND field_name='计算值'",[],|r|r.get(0)).unwrap();
        assert!(matches!(
            serde_json::from_str::<FieldRule>(&saved).unwrap(),
            FieldRule::Calculation(_)
        ));
        assert!(rename_field(&mut db, "欠费表", "金额", "新金额").is_err());
        let (revision, _) = read_rows(&db, "欠费表", 0, 100).unwrap();
        let invalid = RowRule::Divide(Box::new(rule), Box::new(RowRule::Number("0".into())));
        assert!(refresh_field(&mut db, "欠费表", "计算值", &invalid, revision).is_err());
        assert_eq!(
            db.query_row("SELECT 计算值 FROM 欠费表", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "241.00000000"
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM 欠费表", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn decimals_round_and_reject_invalid_values() {
        let db = Connection::open_in_memory().unwrap();
        let number = |s: &str| Rule::Number(s.into());
        let add = Rule::Add(Box::new(number("0.1")), Box::new(number("0.2")));
        assert_eq!(evaluate(&db, &add, "").unwrap().to_string(), "0.30000000");
        let div = Rule::Divide(Box::new(number("-1")), Box::new(number("6")));
        assert_eq!(evaluate(&db, &div, "").unwrap().to_string(), "-0.16666667");
        assert!(evaluate(&db, &number("0.000000001"), "").is_err());
        assert!(evaluate(&db, &number("1e5"), "").is_err());
    }

    #[test]
    fn linked_sum_uses_multiple_keys_without_multiplying_duplicates() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE report_workshop_tables(name TEXT PRIMARY KEY,plan TEXT NOT NULL);INSERT INTO report_workshop_tables VALUES('债务','{}'),('客户','{}');CREATE TABLE 债务(地市 TEXT,客户 TEXT,账期 TEXT,金额 TEXT);CREATE TABLE 客户(客户号 TEXT,账期 TEXT,状态 TEXT);INSERT INTO 债务 VALUES('001','A','202609','100'),('001','B','202609','25');INSERT INTO 客户 VALUES('A','202609','有效'),('A','202609','有效'),('B','202608','有效');").unwrap();
        let rule = Rule::LinkedSum {
            left_table: "债务".into(),
            right_table: "客户".into(),
            amount_field: "金额".into(),
            group_field: "地市".into(),
            keys: vec![
                ("客户".into(), "客户号".into()),
                ("账期".into(), "账期".into()),
            ],
            right_filters: vec![("状态".into(), "有效".into())],
        };
        assert_eq!(evaluate(&db, &rule, "001").unwrap(), "100".parse().unwrap());
        assert!(evaluate(&db, &rule, "002").is_err());
    }

    #[test]
    fn ooxml_string_cells_keep_numeric_text_and_leading_zeroes() {
        use std::io::{Read, Write};
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original.xlsx");
        let source = dir.path().join("text.xlsx");
        let mut book = Workbook::new();
        let sheet = book.add_worksheet();
        sheet.set_name("明细").unwrap();
        sheet.write_string(0, 1, "编号").unwrap();
        sheet.write_string(1, 1, "001").unwrap();
        book.save(&original).unwrap();
        let mut input = zip::ZipArchive::new(std::fs::File::open(original).unwrap()).unwrap();
        let mut output = zip::ZipWriter::new(std::fs::File::create(&source).unwrap());
        let cell = regex::Regex::new(r#"<c r="B2" t="s"><v>\d+</v></c>"#).unwrap();
        for i in 0..input.len() {
            let mut entry = input.by_index(i).unwrap();
            let name = entry.name().to_owned();
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            if name == "xl/worksheets/sheet1.xml" {
                let xml = String::from_utf8(data).unwrap();
                assert!(cell.is_match(&xml));
                data = cell
                    .replace(&xml, r#"<c r="B2" t="str"><v>001</v></c>"#)
                    .as_bytes()
                    .to_vec();
            }
            output
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            output.write_all(&data).unwrap();
        }
        output.finish().unwrap();
        let previews = inspect_workbook(&source).unwrap();
        assert_eq!(previews[0].rows[1][1], "001");
        let plan = SheetImport {
            sheet: "明细".into(),
            table: "明细表".into(),
            first_data_row: 2,
            end_marker: String::new(),
            skip_empty_column: None,
            fields: vec![field("编号", Kind::Text, Source::Column(1))],
        };
        let mut db = Connection::open_in_memory().unwrap();
        import_new_tables(&mut db, &source, &[plan]).unwrap();
        assert_eq!(
            db.query_row("SELECT 编号 FROM 明细表", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "001"
        );
    }

    #[test]
    fn multi_sheet_to_dynamic_report_with_four_operations_and_excel_output() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.xlsx");
        let mut book = Workbook::new();
        let debt = book.add_worksheet();
        debt.set_name("欠费").unwrap();
        debt.write_string(0, 0, "城市").unwrap();
        debt.write_string(0, 1, "金额").unwrap();
        for (r, city, amount) in [(1, "001", 100.25), (2, "001", 49.75), (3, "002", 80.0)] {
            debt.write_string(r, 0, city).unwrap();
            debt.write_number(r, 1, amount).unwrap();
        }
        let paid = book.add_worksheet();
        paid.set_name("回款").unwrap();
        paid.write_string(0, 0, "城市").unwrap();
        paid.write_string(0, 1, "金额").unwrap();
        for (r, city, amount) in [(1, "001", 30.0), (2, "002", 20.0)] {
            paid.write_string(r, 0, city).unwrap();
            paid.write_number(r, 1, amount).unwrap();
        }
        book.save(&path).unwrap();
        let plan = |sheet: &str, table: &str| SheetImport {
            sheet: sheet.into(),
            table: table.into(),
            first_data_row: 2,
            end_marker: String::new(),
            skip_empty_column: None,
            fields: vec![
                field("城市", Kind::Text, Source::Column(0)),
                field("金额", Kind::Decimal, Source::Column(1)),
                field("账期", Kind::Text, Source::Constant("202609".into())),
            ],
        };
        let mut db = Connection::open_in_memory().unwrap();
        assert_eq!(
            import_new_tables(
                &mut db,
                &path,
                &[plan("欠费", "欠费表"), plan("回款", "回款表")]
            )
            .unwrap(),
            vec![3, 2]
        );
        let sum = |table: &str| Rule::Sum {
            table: table.into(),
            field: "金额".into(),
            group_field: "城市".into(),
            filters: vec![("账期".into(), "202609".into())],
            all_rows: false,
        };
        let calc = Rule::Divide(
            Box::new(Rule::Multiply(
                Box::new(Rule::Subtract(
                    Box::new(Rule::Add(
                        Box::new(sum("欠费表")),
                        Box::new(Rule::Number("10".into())),
                    )),
                    Box::new(sum("回款表")),
                )),
                Box::new(Rule::Number("2".into())),
            )),
            Box::new(Rule::Number("4".into())),
        );
        let template = Template {
            title: "动态对账".into(),
            group_table: "欠费表".into(),
            group_field: "城市".into(),
            group_filters: vec![("账期".into(), "202609".into())],
            sort_column: None,
            descending: false,
            limit: None,
            dynamic_row: 2,
            group_column: 0,
            columns: vec![("计算值".into(), calc)],
            cells: vec![
                TemplateCell {
                    row: 3,
                    column: 0,
                    value: CellValue::Text("合计".into()),
                },
                TemplateCell {
                    row: 3,
                    column: 1,
                    value: CellValue::Calculated {
                        group: String::new(),
                        rule: Rule::Subtract(
                            Box::new(Rule::Sum {
                                table: "欠费表".into(),
                                field: "金额".into(),
                                group_field: String::new(),
                                filters: vec![("账期".into(), "202609".into())],
                                all_rows: true,
                            }),
                            Box::new(Rule::Sum {
                                table: "回款表".into(),
                                field: "金额".into(),
                                group_field: String::new(),
                                filters: vec![("账期".into(), "202609".into())],
                                all_rows: true,
                            }),
                        ),
                    },
                },
            ],
        };
        let rows = generate_rows(&db, &template).unwrap();
        assert_eq!(
            rows,
            vec![
                ("001".into(), vec!["65".parse().unwrap()]),
                ("002".into(), vec!["35".parse().unwrap()])
            ]
        );
        let mut top = template.clone();
        top.sort_column = Some(0);
        top.descending = true;
        top.limit = Some(1);
        assert_eq!(
            generate_rows(&db, &top).unwrap(),
            vec![("001".into(), vec!["65".parse().unwrap()])]
        );
        let out = dir.path().join("report.xlsx");
        let cells = generate_cells(&db, &template).unwrap();
        export_report(&out, &template, &rows, &cells).unwrap();
        let version = save_template(&mut db, "demo", &template).unwrap();
        let revision: i64 = db
            .query_row(
                "SELECT value FROM report_workshop_meta WHERE key='data_revision'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        record_export(&mut db, "demo", version, &template, revision, &out).unwrap();
        assert!(record_export(&mut db, "demo", version, &template, revision + 1, &out).is_err());
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM report_workshop_runs", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        let mut result = open_workbook_auto(&out).unwrap();
        let sheet = result.worksheet_range_at(0).unwrap().unwrap();
        assert_eq!(sheet.get((2, 0)).unwrap().to_string(), "001");
        assert_eq!(sheet.get((2, 1)).unwrap().to_string(), "65");
        assert_eq!(sheet.get((3, 1)).unwrap().to_string(), "35");
        assert_eq!(sheet.get((4, 0)).unwrap().to_string(), "合计");
        assert_eq!(sheet.get((4, 1)).unwrap().to_string(), "180");
        let mut shifted = template.clone();
        shifted.dynamic_row = 4;
        shifted.group_column = 2;
        for c in &mut shifted.cells {
            c.row = 5;
        }
        let shifted_cells = generate_cells(&db, &shifted).unwrap();
        let shifted_out = dir.path().join("shifted.xlsx");
        export_report(&shifted_out, &shifted, &rows, &shifted_cells).unwrap();
        let mut shifted_book = open_workbook_auto(&shifted_out).unwrap();
        let shifted_sheet = shifted_book.worksheet_range_at(0).unwrap().unwrap();
        assert_eq!(shifted_sheet.get((4, 2)).unwrap().to_string(), "001");
        assert_eq!(shifted_sheet.get((6, 0)).unwrap().to_string(), "合计");
        assert!(export_report(&out, &template, &rows, &cells).is_err());
        let mut second = template.clone();
        second.title = "第二张表".into();
        let version2 = save_template(&mut db, "demo2", &second).unwrap();
        let book_path = dir.path().join("multi.xlsx");
        export_workbook(
            &book_path,
            &[(&template, &rows, &cells), (&second, &rows, &cells)],
        )
        .unwrap();
        record_workbook_export(
            &mut db,
            &[
                ("demo".into(), version, template.clone()),
                ("demo2".into(), version2, second),
            ],
            revision,
            &book_path,
        )
        .unwrap();
        let book = open_workbook_auto(&book_path).unwrap();
        assert_eq!(
            book.sheet_names(),
            &["动态对账".to_string(), "第二张表".to_string()]
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM report_workshop_runs", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
    }

    #[test]
    #[ignore = "requires the user's real workbook and a dedicated output directory"]
    fn real_tower_workbook_two_sheets_roundtrip() {
        let path =
            std::path::PathBuf::from(std::env::var_os("REPORT_SOURCE").expect("REPORT_SOURCE"));
        let output =
            std::path::PathBuf::from(std::env::var_os("REPORT_OUTPUT").expect("REPORT_OUTPUT"));
        let hash = file_hash(&path).unwrap();
        let plan = |sheet: &str, table: &str, business_col: usize, amount_col: usize| SheetImport {
            sheet: sheet.into(),
            table: table.into(),
            first_data_row: 5,
            end_marker: "[END]".into(),
            skip_empty_column: Some(0),
            fields: vec![
                field("单位编号", Kind::Text, Source::Column(0)),
                field("业务类型", Kind::Text, Source::Column(business_col)),
                field("金额", Kind::Decimal, Source::Column(amount_col)),
                field("账期", Kind::Text, Source::Constant("202609".into())),
            ],
        };
        let plans = vec![
            plan("本年新增坏账金额", "本年坏账", 2, 3),
            plan("坏账准备金额表", "坏账准备", 5, 19),
        ];
        let mut db = Connection::open_in_memory().unwrap();
        assert_eq!(
            import_tables_checked(&mut db, &path, &plans, ImportMode::Create, Some(&hash)).unwrap(),
            vec![23, 304]
        );
        let sum = |table: &str| Rule::Sum {
            table: table.into(),
            field: "金额".into(),
            group_field: "单位编号".into(),
            filters: vec![
                ("业务类型".into(), "一体".into()),
                ("账期".into(), "202609".into()),
            ],
            all_rows: false,
        };
        let calculation = Rule::Divide(
            Box::new(Rule::Multiply(
                Box::new(Rule::Subtract(
                    Box::new(Rule::Add(
                        Box::new(sum("本年坏账")),
                        Box::new(Rule::Number("10".into())),
                    )),
                    Box::new(sum("坏账准备")),
                )),
                Box::new(Rule::Number("2".into())),
            )),
            Box::new(Rule::Number("4".into())),
        );
        let template = Template {
            title: "真实数据跨表计算验证".into(),
            group_table: "本年坏账".into(),
            group_field: "单位编号".into(),
            group_filters: vec![
                ("单位编号".into(), "220100".into()),
                ("业务类型".into(), "一体".into()),
            ],
            sort_column: None,
            descending: false,
            limit: None,
            dynamic_row: 2,
            group_column: 0,
            columns: vec![
                ("本期坏账".into(), sum("本年坏账")),
                ("坏账准备".into(), sum("坏账准备")),
                ("综合计算".into(), calculation),
            ],
            cells: vec![
                TemplateCell {
                    row: 3,
                    column: 0,
                    value: CellValue::Text("测试说明".into()),
                },
                TemplateCell {
                    row: 3,
                    column: 1,
                    value: CellValue::Text("演示算式，不代表业务口径".into()),
                },
            ],
        };
        assert_eq!(
            save_template(&mut db, "真实数据演示", &template).unwrap(),
            1
        );
        let rows = generate_rows(&db, &template).unwrap();
        let cells = generate_cells(&db, &template).unwrap();
        export_report(&output, &template, &rows, &cells).unwrap();
        let mut book = open_workbook_auto(&output).unwrap();
        let sheet = book.worksheet_range_at(0).unwrap().unwrap();
        assert_eq!(sheet.get((2, 0)).unwrap().to_string(), "220100");
        assert_eq!(file_hash(&path).unwrap(), hash);
        eprintln!("rows={rows:?}; output={}", output.display());
    }

    #[test]
    #[ignore = "requires the user's demonstration workbook and a dedicated output path"]
    fn demonstration_workbook_complex_rules() {
        let path =
            std::path::PathBuf::from(std::env::var_os("REPORT_SOURCE").expect("REPORT_SOURCE"));
        let output =
            std::path::PathBuf::from(std::env::var_os("REPORT_OUTPUT").expect("REPORT_OUTPUT"));
        let plan = |sheet: &str, table: &str| SheetImport {
            sheet: sheet.into(),
            table: table.into(),
            first_data_row: 2,
            end_marker: String::new(),
            skip_empty_column: None,
            fields: vec![
                field("地市编码", Kind::Text, Source::Column(0)),
                field("地市", Kind::Text, Source::Column(1)),
                field("客户", Kind::Text, Source::Column(2)),
                field("金额", Kind::Decimal, Source::Column(3)),
                field("账期", Kind::Text, Source::Constant("202609".into())),
            ],
        };
        let mut db = Connection::open_in_memory().unwrap();
        let hash = file_hash(&path).unwrap();
        assert_eq!(
            import_tables_checked(
                &mut db,
                &path,
                &[plan("欠费明细", "欠费"), plan("回款明细", "回款")],
                ImportMode::Create,
                Some(&hash)
            )
            .unwrap(),
            vec![5, 5]
        );
        let sum = |table: &str| Rule::Sum {
            table: table.into(),
            field: "金额".into(),
            group_field: "地市编码".into(),
            filters: vec![("账期".into(), "202609".into())],
            all_rows: false,
        };
        let all = |table: &str| Rule::Sum {
            table: table.into(),
            field: "金额".into(),
            group_field: String::new(),
            filters: vec![("账期".into(), "202609".into())],
            all_rows: true,
        };
        let template = Template {
            title: "跨表四则运算示例".into(),
            group_table: "欠费".into(),
            group_field: "地市编码".into(),
            group_filters: vec![("账期".into(), "202609".into())],
            sort_column: None,
            descending: false,
            limit: None,
            dynamic_row: 2,
            group_column: 0,
            columns: vec![
                ("欠费".into(), sum("欠费")),
                ("回款".into(), sum("回款")),
                (
                    "相加".into(),
                    Rule::Add(Box::new(sum("欠费")), Box::new(sum("回款"))),
                ),
                (
                    "相减".into(),
                    Rule::Subtract(Box::new(sum("欠费")), Box::new(sum("回款"))),
                ),
                (
                    "相乘".into(),
                    Rule::Multiply(Box::new(sum("欠费")), Box::new(sum("回款"))),
                ),
                (
                    "相除".into(),
                    Rule::Divide(Box::new(sum("欠费")), Box::new(sum("回款"))),
                ),
                (
                    "组合计算".into(),
                    Rule::Divide(
                        Box::new(Rule::Multiply(
                            Box::new(Rule::Subtract(
                                Box::new(Rule::Add(
                                    Box::new(sum("欠费")),
                                    Box::new(Rule::Number("10".into())),
                                )),
                                Box::new(sum("回款")),
                            )),
                            Box::new(Rule::Number("2".into())),
                        )),
                        Box::new(Rule::Number("4".into())),
                    ),
                ),
            ],
            cells: vec![
                TemplateCell {
                    row: 3,
                    column: 0,
                    value: CellValue::Text("余额合计".into()),
                },
                TemplateCell {
                    row: 3,
                    column: 1,
                    value: CellValue::Calculated {
                        group: String::new(),
                        rule: Rule::Subtract(Box::new(all("欠费")), Box::new(all("回款"))),
                    },
                },
            ],
        };
        assert_eq!(save_template(&mut db, "演示报表", &template).unwrap(), 1);
        let rows = generate_rows(&db, &template).unwrap();
        let cells = generate_cells(&db, &template).unwrap();
        assert_eq!(rows.len(), 3);
        eprintln!(
            "demo groups={:?}",
            rows.iter().map(|r| &r.0).collect::<Vec<_>>()
        );
        assert_eq!(
            rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
            vec!["001", "002", "003"]
        );
        assert_eq!(
            rows[0]
                .1
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec![
                "1500.00000000",
                "300.00000000",
                "1800.00000000",
                "1200.00000000",
                "450000.00000000",
                "5.00000000",
                "605.00000000"
            ]
        );
        export_report(&output, &template, &rows, &cells).unwrap();
        assert_eq!(file_hash(&path).unwrap(), hash);
    }
}
