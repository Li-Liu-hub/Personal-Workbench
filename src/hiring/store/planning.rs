use super::*;
use crate::hiring::{
    accounts::{MAX_ACCOUNTS, MAX_POOL},
    dedupe::{self, Eligibility},
};
use serde::Serialize;
use std::collections::HashSet;

#[derive(Clone, Debug, Serialize)]
pub struct Assignment {
    pub account_id: String,
    pub account_label: String,
    pub login_checked: bool,
    pub daily_remaining: u32,
    pub jobs: Vec<Job>,
}
#[derive(Clone, Debug, Serialize)]
pub struct PoolPlan {
    pub requested: usize,
    pub assigned: usize,
    pub cooling_down: usize,
    pub needs_review: usize,
    pub duplicate_candidates: usize,
    pub assignments: Vec<Assignment>,
}
impl Store {
    pub fn stop_generation(&self) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT COALESCE(MAX(value),0) FROM guard WHERE key='queue:stop_generation'",
                [],
                |r| r.get(0),
            )
            .map_err(err)
    }
    pub fn request_stop(&mut self) -> Result<i64> {
        self.conn.execute("INSERT INTO guard VALUES('queue:stop_generation',1) ON CONFLICT(key) DO UPDATE SET value=value+1",[]).map_err(err)?;
        self.stop_generation()
    }
    pub fn plan_accounts(
        &mut self,
        ids: &[String],
        limit: usize,
        global_limit: u32,
        commit: bool,
    ) -> Result<PoolPlan> {
        let ids = ids
            .iter()
            .map(|id| crate::hiring::accounts::id(id).map(str::to_owned))
            .collect::<Result<Vec<_>>>()?;
        if ids.is_empty() || ids.len() > MAX_ACCOUNTS || limit == 0 || limit > MAX_POOL {
            return Err("请选择1–3个账号，本轮总量须为1–450".into());
        }
        let unique: HashSet<_> = ids.iter().collect();
        if unique.len() != ids.len() {
            return Err("账号列表不能重复".into());
        }
        let mut assignments = Vec::new();
        for id in &ids {
            let account = self.account(id, global_limit)?;
            if !account.enabled {
                return Err(format!("{}已停用", account.label));
            }
            assignments.push(Assignment {
                account_id: account.id,
                account_label: account.label,
                login_checked: account.login_checked_at > 0,
                daily_remaining: account.remaining,
                jobs: vec![],
            });
        }
        let mut plan = PoolPlan {
            requested: limit,
            assigned: 0,
            cooling_down: 0,
            needs_review: 0,
            duplicate_candidates: 0,
            assignments,
        };
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(err)?;
        let candidates: Vec<(String, String)> = {
            let mut q=tx.prepare("SELECT q.payload,COALESCE(m.account_id,'') FROM queue q LEFT JOIN queue_identity m ON m.url=q.url WHERE q.status='pending' ORDER BY q.priority DESC,q.rowid").map_err(err)?;
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(err)?
                .collect::<std::result::Result<_, _>>()
                .map_err(err)?
        };
        let mut fingerprints = HashSet::new();
        let mut next = 0;
        for (payload, assigned) in candidates {
            if plan.assigned >= limit {
                break;
            }
            if !assigned.is_empty() && !ids.contains(&assigned) {
                continue;
            }
            let mut job: Job = serde_json::from_str(&payload).map_err(err)?;
            match Self::eligibility_at(&tx, "boss", &job, Local::now().timestamp())? {
                Eligibility::CoolingDown { .. } => {
                    plan.cooling_down += 1;
                    continue;
                }
                Eligibility::NeedsReview { .. } => {
                    plan.needs_review += 1;
                    continue;
                }
                Eligibility::Eligible => {}
            }
            let fp = dedupe::fingerprint(&job).unwrap_or_default();
            if !fp.is_empty() && !fingerprints.insert(fp.clone()) {
                plan.duplicate_candidates += 1;
                continue;
            }
            let target = (0..plan.assignments.len())
                .map(|offset| (next + offset) % plan.assignments.len())
                .find(|i| {
                    plan.assignments[*i].jobs.len() < plan.assignments[*i].daily_remaining as usize
                });
            let Some(index) = target else {
                break;
            };
            next = (index + 1) % plan.assignments.len();
            job.account_id = plan.assignments[index].account_id.clone();
            if commit {
                tx.execute("INSERT INTO queue_identity VALUES(?1,?2,?3,?4) ON CONFLICT(url) DO UPDATE SET account_id=excluded.account_id,fingerprint=excluded.fingerprint",params![job.url,fp,job.account_id,Local::now().timestamp()]).map_err(err)?;
            }
            plan.assignments[index].jobs.push(job);
            plan.assigned += 1;
        }
        tx.commit().map_err(err)?;
        Ok(plan)
    }
    pub fn next_pending(&mut self, visited: &HashSet<String>) -> Result<Option<Job>> {
        let account_id = self.account_id.clone();
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(err)?;
        let rows: Vec<String> = {
            let mut q=tx.prepare("SELECT q.payload FROM queue q LEFT JOIN queue_identity m ON m.url=q.url WHERE q.status='pending' AND (COALESCE(m.account_id,'')='' OR m.account_id=?1) ORDER BY q.priority DESC,q.rowid").map_err(err)?;
            q.query_map([&account_id], |r| r.get(0))
                .map_err(err)?
                .collect::<std::result::Result<_, _>>()
                .map_err(err)?
        };
        let mut selected = None;
        for payload in rows {
            let mut job: Job = serde_json::from_str(&payload).map_err(err)?;
            if visited.contains(&job.url) {
                continue;
            }
            let fp = dedupe::fingerprint(&job).unwrap_or_default();
            tx.execute("INSERT INTO queue_identity VALUES(?1,?2,?3,?4) ON CONFLICT(url) DO UPDATE SET account_id=excluded.account_id",params![job.url,fp,account_id,Local::now().timestamp()]).map_err(err)?;
            job.account_id = account_id.clone();
            selected = Some(job);
            break;
        }
        tx.commit().map_err(err)?;
        Ok(selected)
    }
    pub fn latest_attempt(&self, site: &str, url: &str) -> Result<Option<(i64, String)>> {
        self.conn
            .query_row(
                "SELECT id,status FROM attempts WHERE site=?1 AND url=?2 ORDER BY id DESC LIMIT 1",
                params![site, url],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(err)
    }
}
