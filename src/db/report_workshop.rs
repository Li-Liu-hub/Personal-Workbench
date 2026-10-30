use crate::{
    db::Database,
    error::Result,
    report_workshop::{self, DataPage, Field, FieldRule, RunInfo, SheetImport, Template},
};
use rusqlite::OptionalExtension;

impl Database {
    pub fn report_save_field_rule(
        &mut self,
        table: &str,
        field: &str,
        rule: &FieldRule,
    ) -> Result<()> {
        report_workshop::save_field_rule(&mut self.desk, table, field, rule)
    }
    pub fn report_field_rule(&self, table: &str, field: &str) -> Result<Option<FieldRule>> {
        if !self
            .desk
            .table_exists(None, "report_workshop_field_rules")?
        {
            return Ok(None);
        }
        let raw:Option<String>=self.desk.query_row("SELECT rule FROM report_workshop_field_rules WHERE table_name=?1 AND field_name=?2",rusqlite::params![table,field],|r|r.get(0)).optional()?;
        raw.map(|json| serde_json::from_str(&json).map_err(Into::into))
            .transpose()
    }
    pub fn report_runs(&self) -> Result<Vec<RunInfo>> {
        if !self.desk.table_exists(None, "report_workshop_runs")? {
            return Ok(Vec::new());
        }
        let mut stmt=self.desk.prepare("SELECT template_id,version,data_revision,output_path,sha256,created_at FROM report_workshop_runs ORDER BY rowid DESC LIMIT 100")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }
    pub fn report_record_export(
        &mut self,
        id: &str,
        version: i64,
        template: &Template,
        revision: i64,
        path: &std::path::Path,
    ) -> Result<()> {
        report_workshop::record_export(&mut self.desk, id, version, template, revision, path)
    }
    pub fn report_rename_field(&mut self, table: &str, old: &str, new: &str) -> Result<()> {
        report_workshop::rename_field(&mut self.desk, table, old, new)
    }
    pub fn report_read_rows(&self, table: &str, offset: usize) -> Result<DataPage> {
        let tx = self.desk.unchecked_transaction()?;
        let result = report_workshop::read_rows(&tx, table, offset, 100)?;
        tx.commit()?;
        Ok(result)
    }
    pub fn report_update_cell(
        &mut self,
        table: &str,
        rowid: i64,
        field: &str,
        value: &str,
        revision: i64,
    ) -> Result<()> {
        report_workshop::update_cell(&mut self.desk, table, rowid, field, value, revision)
    }
    pub fn report_add_field(&mut self, table: &str, field: Field) -> Result<()> {
        report_workshop::add_field(&mut self.desk, table, field)
    }
    pub fn report_revision(&self) -> Result<i64> {
        if !self.desk.table_exists(None, "report_workshop_meta")? {
            return Ok(0);
        }
        Ok(self.desk.query_row(
            "SELECT value FROM report_workshop_meta WHERE key='data_revision'",
            [],
            |r| r.get(0),
        )?)
    }
    pub fn report_tables(&self) -> Result<Vec<SheetImport>> {
        if !self.desk.table_exists(None, "report_workshop_tables")? {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .desk
            .prepare("SELECT plan FROM report_workshop_tables ORDER BY name")?;
        let raw = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|json| Ok(serde_json::from_str(&json)?))
            .collect()
    }

    pub fn report_save_template(&mut self, id: &str, template: &Template) -> Result<i64> {
        report_workshop::save_template(&mut self.desk, id, template)
    }

    pub fn report_templates(&self) -> Result<Vec<(String, i64, Template)>> {
        if !self.desk.table_exists(None, "report_workshop_templates")? {
            return Ok(Vec::new());
        }
        let mut stmt = self.desk.prepare("SELECT template_id,version,content FROM report_workshop_templates ORDER BY template_id,version DESC")?;
        let raw = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|(id, version, json)| Ok((id, version, serde_json::from_str(&json)?)))
            .collect()
    }
}
