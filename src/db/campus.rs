use super::*;
use crate::campus::Job;

impl Database {
    pub(super) fn init_campus(&self) -> Result<()> {
        self.desk.execute_batch(
            "CREATE TABLE IF NOT EXISTS campus_jobs(
          id TEXT PRIMARY KEY, payload TEXT NOT NULL,
          favorite INTEGER NOT NULL DEFAULT 0, applied INTEGER NOT NULL DEFAULT 0,
          ignored INTEGER NOT NULL DEFAULT 0, first_seen TEXT NOT NULL,
          checked_at TEXT NOT NULL
        ); CREATE INDEX IF NOT EXISTS campus_jobs_checked ON campus_jobs(checked_at);",
        )?;
        Ok(())
    }
    pub fn campus_jobs(&self) -> Result<Vec<Job>> {
        let mut statement = self.desk.prepare("SELECT payload,favorite,applied,ignored,first_seen FROM campus_jobs ORDER BY checked_at DESC,id")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, bool>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, bool>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut jobs = Vec::new();
        for row in rows {
            let (payload, favorite, applied, ignored, first_seen) = row?;
            let mut job: Job = serde_json::from_str(&payload)?;
            job.favorite = favorite;
            job.applied = applied;
            job.ignored = ignored;
            job.first_seen = first_seen;
            jobs.push(job);
        }
        Ok(jobs)
    }
    pub fn save_campus_jobs(&mut self, jobs: &[Job]) -> Result<()> {
        let tx = self.desk.transaction()?;
        for job in jobs {
            tx.execute("INSERT INTO campus_jobs(id,payload,favorite,applied,ignored,first_seen,checked_at) VALUES(?1,?2,?3,?4,?5,?6,?7)
              ON CONFLICT(id) DO UPDATE SET payload=excluded.payload,checked_at=excluded.checked_at",
              params![job.id,serde_json::to_string(job)?,job.favorite,job.applied,job.ignored,job.first_seen,job.checked_at])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn set_campus_flags(&mut self, job: &Job) -> Result<()> {
        self.desk.execute(
            "UPDATE campus_jobs SET favorite=?2,applied=?3,ignored=?4 WHERE id=?1",
            params![job.id, job.favorite, job.applied, job.ignored],
        )?;
        Ok(())
    }
}
