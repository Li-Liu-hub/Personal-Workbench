use super::*;
use crate::hiring::{
    BOSS_MAX_DAILY,
    accounts::{self, Account, DEFAULT_ACCOUNT, MAX_ACCOUNTS},
};

impl Store {
    pub fn account(&self, id: &str, global_limit: u32) -> Result<Account> {
        let id = accounts::id(id)?;
        let mut account=self.conn.query_row("SELECT id,label,slot,daily_limit,enabled,login_checked_at FROM hiring_accounts WHERE id=?1",[id],|r|Ok(Account{id:r.get(0)?,label:r.get(1)?,slot:r.get(2)?,daily_limit:r.get(3)?,enabled:r.get(4)?,login_checked_at:r.get(5)?,today_attempts:0,today_sent:0,remaining:0})).map_err(|_|"账号不存在，请先创建或选择已有账号".to_owned())?;
        let day = Local::now().format("%Y-%m-%d").to_string();
        let (attempts, sent) = self.account_usage("boss", id, &day)?;
        account.today_attempts = attempts;
        account.today_sent = sent;
        account.remaining = global_limit
            .min(account.daily_limit)
            .min(BOSS_MAX_DAILY)
            .saturating_sub(attempts);
        Ok(account)
    }
    pub fn accounts(&self, global_limit: u32) -> Result<Vec<Account>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM hiring_accounts ORDER BY slot")
            .map_err(err)?;
        let ids = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        ids.iter()
            .map(|id| self.account(id, global_limit))
            .collect()
    }
    pub fn create_account(&mut self, label: &str) -> Result<Account> {
        let label = accounts::label(label)?;
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(err)?;
        let mut stmt = tx
            .prepare("SELECT slot FROM hiring_accounts ORDER BY slot")
            .map_err(err)?;
        let occupied = stmt
            .query_map([], |r| r.get::<_, u16>(0))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        drop(stmt);
        let slot = (0..MAX_ACCOUNTS as u16)
            .find(|s| !occupied.contains(s))
            .ok_or("最多配置三个独立账号；停用账号仍保留记录及额度计数")?;
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO hiring_accounts VALUES(?1,?2,?3,150,1,0,?4)",
            params![id, label, slot, Local::now().timestamp()],
        )
        .map_err(|_| "账号名称已存在或无法创建".to_owned())?;
        tx.commit().map_err(err)?;
        self.account(&id, BOSS_MAX_DAILY)
    }
    pub fn update_account(
        &mut self,
        id: &str,
        label: Option<&str>,
        enabled: Option<bool>,
        limit: Option<u32>,
    ) -> Result<Account> {
        let old = self.account(id, BOSS_MAX_DAILY)?;
        let label = accounts::label(label.unwrap_or(&old.label))?;
        let limit = limit.unwrap_or(old.daily_limit);
        if limit == 0 || limit > BOSS_MAX_DAILY {
            return Err("每个账号的每日上限须为 1–150".into());
        }
        self.conn
            .execute(
                "UPDATE hiring_accounts SET label=?2,enabled=?3,daily_limit=?4 WHERE id=?1",
                params![old.id, label, enabled.unwrap_or(old.enabled), limit],
            )
            .map_err(|_| "账号名称已存在或无法保存".to_owned())?;
        self.account(&old.id, BOSS_MAX_DAILY)
    }
    pub fn mark_login(&mut self, id: &str, logged_in: bool) -> Result<()> {
        let id = accounts::id(id)?;
        let changed = self
            .conn
            .execute(
                "UPDATE hiring_accounts SET login_checked_at=?2 WHERE id=?1",
                params![
                    id,
                    if logged_in {
                        Local::now().timestamp()
                    } else {
                        0
                    }
                ],
            )
            .map_err(err)?;
        if changed != 1 {
            return Err("账号不存在".into());
        }
        Ok(())
    }
    pub fn account_usage(&self, site: &str, id: &str, day: &str) -> Result<(u32, u32)> {
        let (count,sent):(u32,u32)=self.conn.query_row("SELECT COUNT(*),COALESCE(SUM(a.status='sent'),0) FROM attempts a LEFT JOIN attempt_identity m ON m.attempt_id=a.id WHERE a.site=?1 AND a.day=?2 AND COALESCE(m.account_id,'default')=?3",params![site,day,id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(err)?;
        let key = if id == DEFAULT_ACCOUNT {
            format!("{site}:imported:{day}")
        } else {
            format!("{site}:{id}:imported:{day}")
        };
        let imported: u32 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(value),0) FROM guard WHERE key=?1",
                [key],
                |r| r.get(0),
            )
            .map_err(err)?;
        Ok((count.saturating_add(imported), sent))
    }
}
