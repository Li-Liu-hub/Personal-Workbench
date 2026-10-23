use super::*;
use crate::games::records::Library;
impl Database {
    pub(super) fn prepare_games(&self) -> Result<()> {
        self.desk.execute_batch("CREATE TABLE IF NOT EXISTS game_runs(run_id TEXT PRIMARY KEY,at TEXT NOT NULL,record_json TEXT NOT NULL); CREATE INDEX IF NOT EXISTS game_runs_time ON game_runs(at DESC);")?;
        Ok(())
    }
    pub fn game_records(&self, limit: u32) -> Result<Vec<crate::games::records::Record>> {
        let rows = self
            .desk
            .prepare_cached(
                "SELECT record_json FROM game_runs ORDER BY at DESC,rowid DESC LIMIT ?1",
            )?
            .query_map([i64::from(limit)], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|json| Ok(serde_json::from_str(&json)?))
            .collect()
    }
    pub fn game_library(&self) -> Result<Library> {
        let mut library = if let Some(json) = self.setting("games.library.v1")? {
            serde_json::from_str::<Library>(&json)?
        } else {
            Library::default()
        };
        library.validate().map_err(DeskError::InvalidInput)?;
        let rows = self.game_records(100)?;
        if !rows.is_empty() {
            library.records = rows;
        }
        library.snake_best = library.snake_best.max(
            self.setting("games.snake.best")?
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
        );
        if let Some(saved) = &mut library.snake {
            saved.game.pause();
        }
        if let Some(saved) = &mut library.tetris {
            saved.game.pause();
        }
        Ok(library)
    }
    pub fn save_game_library(&mut self, library: &Library) -> Result<()> {
        library.validate().map_err(DeskError::InvalidInput)?;
        let tx = self.desk.transaction()?;
        for record in &library.records {
            tx.execute(
                "INSERT OR IGNORE INTO game_runs(run_id,at,record_json) VALUES(?1,?2,?3)",
                params![record.run_id, record.at, serde_json::to_string(record)?],
            )?;
        }
        for (key, value) in [
            ("games.library.v1", serde_json::to_string(library)?),
            ("games.snake.best", library.snake_best.to_string()),
        ] {
            tx.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value])?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn older_runs_remain_append_only_after_the_recent_cache_rolls_over() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Database::open(dir.path()).unwrap();
        let mut library = db.game_library().unwrap();
        for i in 0..125 {
            library.record(crate::games::records::Record {
                run_id: format!("history-{i}"),
                game: "snake".into(),
                stage: 1,
                score: 100 + i,
                lines: 0,
                outcome: "通关".into(),
                at: format!("2026-10-23T{i:04}"),
            });
            db.save_game_library(&library).unwrap();
        }
        assert_eq!(library.records.len(), 100);
        drop(db);
        let db = Database::open(dir.path()).unwrap();
        assert_eq!(db.game_records(u32::MAX).unwrap().len(), 125);
        assert!(
            db.game_records(u32::MAX)
                .unwrap()
                .iter()
                .any(|r| r.run_id == "history-0")
        );
        assert_eq!(db.game_library().unwrap().stage_best["snake:1"], 224);
    }
    use crate::games::{
        self,
        records::{Record, Saved},
    };
    #[test]
    fn saves_resume_and_runs_unlock_once_without_losing_legacy_scores() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Database::open(dir.path()).unwrap();
        db.set_setting("games.snake.best", "90").unwrap();
        let mut library = db.game_library().unwrap();
        assert_eq!(library.snake_best, 90);
        let mut snake = games::Snake::stage(1, 1);
        snake.toggle();
        snake.step();
        library.snake = Some(Saved {
            run_id: "run-s".into(),
            game: snake,
        });
        library.tetris = Some(Saved {
            run_id: "run-t".into(),
            game: games::tetris::Tetris::new(3, 2),
        });
        let r = Record {
            run_id: "run-done".into(),
            game: "snake".into(),
            stage: 1,
            score: 50,
            lines: 0,
            outcome: "通关".into(),
            at: "2026-10-23".into(),
        };
        library.record(r.clone());
        library.record(r);
        db.save_game_library(&library).unwrap();
        drop(db);
        let mut db = Database::open(dir.path()).unwrap();
        let restored = db.game_library().unwrap();
        assert_eq!(restored.snake_unlocked, 2);
        assert_eq!(restored.records.len(), 1);
        assert_eq!(restored.snake_best, 90);
        assert_eq!(
            restored.snake.as_ref().unwrap().game.status,
            games::Status::Paused
        );
        assert!(restored.tetris.as_ref().unwrap().game.valid());
        let mut bad = restored.clone();
        bad.snake.as_mut().unwrap().game.width = 100;
        assert!(db.save_game_library(&bad).is_err());
        assert_eq!(db.game_library().unwrap().snake_best, 90);
        for scope in [crate::export::Scope::All, crate::export::Scope::Games] {
            let zip_path = dir.path().join(format!("{scope:?}.zip"));
            crate::export::write(dir.path(), &zip_path, scope).unwrap();
            let mut zip = zip::ZipArchive::new(std::fs::File::open(zip_path).unwrap()).unwrap();
            let value: serde_json::Value =
                serde_json::from_reader(zip.by_name("小游戏.json").unwrap()).unwrap();
            assert_eq!(value["library"]["records"][0]["run_id"], "run-done");
        }
    }
}
