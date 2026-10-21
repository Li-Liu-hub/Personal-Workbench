//! Disposable SQLite exercises. This module never receives a path to desk.db.
use rusqlite::{
    Batch, Connection,
    fallible_iterator::FallibleIterator,
    hooks::{AuthAction, AuthContext, Authorization},
    limits::Limit,
    types::ValueRef,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, String>;
const MAX_ROWS: usize = 200;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lab {
    pub setup: String,
    pub tables: Vec<String>,
    pub reference: String,
    /// For mutations, inspect the resulting state before rolling it back.
    #[serde(default)]
    pub verify: Option<String>,
    #[serde(default)]
    pub ordered: bool,
    #[serde(default = "yes")]
    pub runnable: bool,
    pub explanation: String,
}
fn yes() -> bool {
    true
}
impl Lab {
    pub fn validate(&self) -> Result<()> {
        if self.setup.trim().is_empty()
            || self.setup.len() > 65_536
            || self.reference.trim().is_empty()
            || self.reference.len() > 65_536
            || self.tables.is_empty()
            || self.tables.len() > 12
            || self
                .tables
                .iter()
                .any(|s| s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
            || self.verify.as_ref().is_some_and(|s| s.len() > 65_536)
        {
            return Err("SQL 练习需要建表数据、表名和参考 SQL，单段不超过 64 KiB".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Table {
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub truncated: bool,
}
#[derive(Clone, Debug)]
pub struct Preview {
    pub tables: Vec<Table>,
    pub expected: Table,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Report {
    pub results: Vec<Table>,
    pub actual: Option<Table>,
    pub expected: Option<Table>,
    pub matches: Option<bool>,
    pub changed_rows: u64,
    pub error: Option<String>,
    pub elapsed_ms: u64,
    pub rolled_back: bool,
}
impl Report {
    pub fn model_context(&self) -> Value {
        let compact = |table: &Option<Table>| {
            table.as_ref().map(|t| {
                serde_json::json!({"columns":t.columns,"rows":t.rows.iter().take(20).collect::<Vec<_>>(),
                    "truncated":t.truncated || t.rows.len()>20})
            })
        };
        serde_json::json!({"dialect":"SQLite","error":self.error,"sample_matches":self.matches,
            "changed_rows":self.changed_rows,"actual":compact(&self.actual),
            "expected":compact(&self.expected),"restored":self.rolled_back})
    }
}

fn authorize(context: AuthContext<'_>) -> Authorization {
    use AuthAction::*;
    match context.action {
        Attach { .. }
        | Detach { .. }
        | Pragma { .. }
        | CreateVtable { .. }
        | DropVtable { .. }
        | Unknown { .. } => Authorization::Deny,
        Transaction { .. } | Savepoint { .. } => Authorization::Deny,
        Function { function_name }
            if matches!(
                function_name.to_ascii_lowercase().as_str(),
                "load_extension" | "writefile" | "readfile" | "edit" | "fts3_tokenizer"
            ) =>
        {
            Authorization::Deny
        }
        _ => Authorization::Allow,
    }
}
fn budget(conn: &Connection) -> Result<()> {
    let started = Instant::now();
    let mut calls = 0;
    conn.progress_handler(
        1000,
        Some(move || {
            calls += 1;
            calls > 2000 || started.elapsed() > Duration::from_secs(2)
        }),
    )
    .map_err(|e| e.to_string())
}
fn connection(lab: &Lab) -> Result<Connection> {
    lab.validate()?;
    let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
    conn.execute_batch(
        "PRAGMA foreign_keys=ON; PRAGMA temp_store=MEMORY; PRAGMA max_page_count=4096;",
    )
    .map_err(|e| e.to_string())?;
    for (limit, value) in [
        (Limit::SQLITE_LIMIT_LENGTH, 262_144),
        (Limit::SQLITE_LIMIT_SQL_LENGTH, 65_536),
        (Limit::SQLITE_LIMIT_COLUMN, 64),
        (Limit::SQLITE_LIMIT_ATTACHED, 0),
        (Limit::SQLITE_LIMIT_TRIGGER_DEPTH, 16),
    ] {
        conn.set_limit(limit, value).map_err(|e| e.to_string())?;
    }
    // Imports use the same disposable engine, never a filesystem connection.
    conn.authorizer(Some(authorize))
        .map_err(|e| e.to_string())?;
    budget(&conn)?;
    execute(&conn, &lab.setup)?;
    Ok(conn)
}
fn values(stmt: &mut rusqlite::Statement<'_>, title: String) -> Result<Table> {
    let columns = stmt.column_names().into_iter().map(str::to_owned).collect();
    let count = stmt.column_count();
    let mut table = Table {
        title,
        columns,
        ..Default::default()
    };
    let mut cursor = stmt.query([]).map_err(|e| e.to_string())?;
    while let Some(row) = cursor.next().map_err(|e| e.to_string())? {
        if table.rows.len() == MAX_ROWS {
            // Do not report a partly executed INSERT ... RETURNING as success.
            return Err(format!(
                "结果超过 {MAX_ROWS} 行，请缩小查询范围；本次改动已撤回"
            ));
        }
        let mut fields = Vec::with_capacity(count);
        for i in 0..count {
            let value = match row.get_ref(i).map_err(|e| e.to_string())? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(v) => Value::from(v),
                ValueRef::Real(v) => Value::from(v),
                ValueRef::Text(v) => {
                    let text = String::from_utf8_lossy(v);
                    if text.chars().count() > 4096 {
                        table.truncated = true;
                    }
                    Value::String(text.chars().take(4096).collect())
                }
                ValueRef::Blob(v) => {
                    if v.len() > 2048 {
                        table.truncated = true;
                    }
                    serde_json::json!({"blob":v.iter().take(2048).map(|b|format!("{b:02x}")).collect::<String>()})
                }
            };
            fields.push(value);
        }
        table.rows.push(fields);
    }
    Ok(table)
}
fn execute(conn: &Connection, sql: &str) -> Result<Vec<Table>> {
    if sql.len() > 65_536 {
        return Err("SQL 请控制在 64 KiB 以内".into());
    }
    let mut batch = Batch::new(conn, sql);
    let mut tables = Vec::new();
    let mut count = 0;
    while let Some(mut stmt) = batch.next().map_err(|e| e.to_string())? {
        count += 1;
        if count > 40 {
            return Err("一次最多执行 40 条 SQL".into());
        }
        if stmt.column_count() == 0 {
            stmt.execute([]).map_err(|e| e.to_string())?;
        } else {
            tables.push(values(&mut stmt, format!("语句 {count}"))?);
        }
    }
    if count == 0 {
        return Err("请先填写 SQL".into());
    }
    Ok(tables)
}
fn query(conn: &Connection, sql: &str, title: &str) -> Result<Table> {
    let mut statement = conn.prepare(sql).map_err(|e| e.to_string())?;
    if !statement.readonly() || statement.column_count() == 0 {
        return Err("验证语句必须是单条只读查询".into());
    }
    values(&mut statement, title.into())
}
fn expected(lab: &Lab) -> Result<Table> {
    let conn = connection(lab)?;
    budget(&conn)?;
    let results = execute(&conn, &lab.reference)?;
    if let Some(verify) = &lab.verify {
        query(&conn, verify, "预期结果")
    } else {
        results
            .last()
            .cloned()
            .ok_or("参考 SQL 需要返回查询结果".into())
    }
}
pub fn preview(lab: &Lab) -> Result<Preview> {
    let conn = connection(lab)?;
    let mut tables = Vec::new();
    for name in &lab.tables {
        tables.push(query(&conn, &format!("SELECT * FROM \"{name}\""), name)?);
    }
    Ok(Preview {
        tables,
        expected: expected(lab)?,
    })
}
fn equal(a: &Table, b: &Table, ordered: bool) -> bool {
    if a.truncated
        || b.truncated
        || a.columns.len() != b.columns.len()
        || a.rows.len() != b.rows.len()
    {
        return false;
    }
    let normalize = |rows: &[Vec<Value>]| {
        let mut rows = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|v| {
                        if let Some(n) = v.as_i64() {
                            format!("number:{n}")
                        } else if let Some(n) = v.as_f64() {
                            format!("number:{n}")
                        } else {
                            v.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        if !ordered {
            rows.sort();
        }
        rows
    };
    normalize(&a.rows) == normalize(&b.rows)
}
pub fn run(lab: &Lab, sql: &str) -> Report {
    let started = Instant::now();
    let mut report = Report::default();
    let result = (|| -> Result<()> {
        if !lab.runnable {
            return Err("这是一道概念题，请填写文字解答".into());
        }
        report.expected = Some(expected(lab)?);
        let conn = connection(lab)?;
        conn.authorizer(None::<fn(AuthContext<'_>) -> Authorization>)
            .map_err(|e| e.to_string())?;
        conn.execute_batch("SAVEPOINT personaldesk_attempt")
            .map_err(|e| e.to_string())?;
        conn.authorizer(Some(authorize))
            .map_err(|e| e.to_string())?;
        budget(&conn)?;
        let before = conn.total_changes();
        let outcome = (|| -> Result<()> {
            report.results = execute(&conn, sql)?;
            report.actual = if let Some(verify) = &lab.verify {
                Some(query(&conn, verify, "执行后的数据")?)
            } else {
                report.results.last().cloned()
            };
            report.matches = Some(report.actual.as_ref().is_some_and(|actual| {
                equal(actual, report.expected.as_ref().unwrap(), lab.ordered)
            }));
            Ok(())
        })();
        report.changed_rows = conn.total_changes() - before;
        conn.progress_handler(0, None::<fn() -> bool>)
            .map_err(|e| e.to_string())?;
        conn.authorizer(None::<fn(AuthContext<'_>) -> Authorization>)
            .map_err(|e| e.to_string())?;
        if !conn.is_autocommit() {
            conn.execute_batch("ROLLBACK TO personaldesk_attempt; RELEASE personaldesk_attempt;")
                .map_err(|e| e.to_string())?;
        }
        // Even ON CONFLICT ROLLBACK cannot persist anything: the connection is memory-only.
        report.rolled_back = true;
        outcome
    })();
    if let Err(error) = result {
        report.error = Some(
            if error.contains("not authorized") || error.contains("authorization denied") {
                "练习自动管理事务，只运行本题表上的 SQL；不需要 BEGIN、COMMIT、PRAGMA 或连接外部数据库。".into()
            } else if error.contains("interrupted") {
                "SQL 运行超过计算预算，已停止并还原数据；请检查递归或查询范围。".into()
            } else {
                error
            },
        );
    }
    // A connection that failed during setup or execution is dropped as well.
    report.rolled_back = true;
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    report
}

#[cfg(test)]
mod tests;
