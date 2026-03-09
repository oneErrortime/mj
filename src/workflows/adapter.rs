//! SQLite-backed job queue adapter for the workflows module.
//!
//! Schema:
//! ```sql
//! CREATE TABLE jobs (
//!     id           TEXT PRIMARY KEY,
//!     workflow     TEXT NOT NULL,
//!     status       TEXT NOT NULL,
//!     doc          TEXT NOT NULL   -- full Job JSON blob
//! );
//! CREATE TABLE signals (
//!     id      TEXT PRIMARY KEY,   -- signal_name + ':' + key
//!     payload TEXT,
//!     exp_at  INTEGER             -- expiry epoch-ms (NULL = no expiry)
//! );
//! ```

use super::{CreateJobOptions, Job, JobStatus};
use crate::error::{MoleculerError, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct SqliteJobAdapter {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteJobAdapter {
    pub fn memory() -> Self {
        let conn = Connection::open_in_memory().expect("in-memory SQLite");
        let a = Self { conn: Arc::new(Mutex::new(conn)) };
        a.init_schema().expect("schema init");
        a
    }

    pub fn file(path: &str) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| MoleculerError::Internal(format!("SQLite: {e}")))?;
        let a = Self { conn: Arc::new(Mutex::new(conn)) };
        a.init_schema()?;
        Ok(a)
    }

    fn init_schema(&self) -> Result<()> {
        let c = self.conn.lock().unwrap();
        c.execute_batch(
            "CREATE TABLE IF NOT EXISTS jobs (
                id       TEXT PRIMARY KEY,
                workflow TEXT NOT NULL,
                status   TEXT NOT NULL,
                doc      TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS jobs_wf_status ON jobs(workflow, status);
             CREATE INDEX IF NOT EXISTS jobs_promote ON jobs(status, doc);

             CREATE TABLE IF NOT EXISTS signals (
                id     TEXT PRIMARY KEY,
                payload TEXT,
                exp_at  INTEGER
             );

             CREATE TABLE IF NOT EXISTS job_events (
                id         TEXT PRIMARY KEY,
                workflow   TEXT NOT NULL,
                job_id     TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload    TEXT,
                ts         INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS je_job ON job_events(workflow, job_id);"
        ).map_err(|e| MoleculerError::Internal(format!("schema: {e}")))
    }

    fn save_job(&self, job: &Job) -> Result<()> {
        let blob = serde_json::to_string(job)
            .map_err(|e| MoleculerError::Internal(e.to_string()))?;
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT OR REPLACE INTO jobs (id, workflow, status, doc) VALUES (?1,?2,?3,?4)",
            params![job.id, job.workflow, job.status.to_string(), blob],
        ).map_err(|e| MoleculerError::Internal(format!("save_job: {e}")))?;
        Ok(())
    }

    fn load_job_from_blob(blob: &str) -> Result<Job> {
        serde_json::from_str(blob)
            .map_err(|e| MoleculerError::Internal(format!("job parse: {e}")))
    }

    // ─── Public API ───────────────────────────────────────────────────────────

    pub fn create_job(
        &self,
        workflow: &str,
        params:   Value,
        opts:     CreateJobOptions,
    ) -> Result<Job> {
        let now = Utc::now().timestamp_millis();
        let id  = opts.job_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let delay_ms = opts.delay.unwrap_or(0);
        let (status, promote_at) = if delay_ms > 0 {
            (JobStatus::Delayed, Some(now + delay_ms))
        } else {
            (JobStatus::Waiting, None)
        };

        let job = Job {
            id: id.clone(),
            workflow:    workflow.to_string(),
            params,
            status,
            retries:     0,
            max_retries: opts.retries.unwrap_or(3),
            delay_ms,
            promote_at,
            started_at:  None,
            finished_at: None,
            result:      None,
            error:       None,
            parent_id:   opts.parent_id,
            progress:    0.0,
            created_at:  now,
            updated_at:  now,
            cron:        opts.cron,
            repeat_limit: opts.repeat_limit,
            repeat_count: 0,
            stalled_count: 0,
        };

        self.save_job(&job)?;
        Ok(job)
    }

    pub fn get_job(&self, workflow: &str, id: &str) -> Result<Option<Job>> {
        let c = self.conn.lock().unwrap();
        let mut s = c.prepare(
            "SELECT doc FROM jobs WHERE id = ?1 AND workflow = ?2"
        ).map_err(|e| MoleculerError::Internal(e.to_string()))?;
        let mut rows = s.query(params![id, workflow])
            .map_err(|e| MoleculerError::Internal(e.to_string()))?;
        if let Some(row) = rows.next().map_err(|e| MoleculerError::Internal(e.to_string()))? {
            let blob: String = row.get(0).map_err(|e| MoleculerError::Internal(e.to_string()))?;
            return Ok(Some(Self::load_job_from_blob(&blob)?));
        }
        Ok(None)
    }

    /// Pull next waiting job for `workflow`, mark it Active. Returns None if queue empty.
    pub fn claim_next_job(&self, workflow: &str) -> Result<Option<Job>> {
        let now = Utc::now().timestamp_millis();
        // Promote delayed jobs whose time has come
        {
            let c = self.conn.lock().unwrap();
            let blobs: Vec<String> = {
                let mut s = c.prepare(
                    "SELECT doc FROM jobs WHERE workflow=?1 AND status='delayed'"
                ).map_err(|e| MoleculerError::Internal(e.to_string()))?;
                let rows = s.query_map(params![workflow], |r| r.get::<_, String>(0))
                    .map_err(|e| MoleculerError::Internal(e.to_string()))?;
                rows.filter_map(|r| r.ok()).collect()
            };
            for blob in blobs {
                if let Ok(mut j) = Self::load_job_from_blob(&blob) {
                    if j.promote_at.map_or(false, |t| t <= now) {
                        j.status = JobStatus::Waiting;
                        j.promote_at = None;
                        let new_blob = serde_json::to_string(&j).unwrap_or_default();
                        let _ = c.execute(
                            "UPDATE jobs SET status='waiting', doc=?1 WHERE id=?2",
                            params![new_blob, j.id],
                        );
                    }
                }
            }
        }

        // Claim oldest waiting job
        let blob_opt: Option<String> = {
            let c = self.conn.lock().unwrap();
            let mut s = c.prepare(
                "SELECT doc FROM jobs WHERE workflow=?1 AND status='waiting'
                 ORDER BY json_extract(doc,'$.created_at') ASC LIMIT 1"
            ).map_err(|e| MoleculerError::Internal(e.to_string()))?;
            let mut rows = s.query(params![workflow])
                .map_err(|e| MoleculerError::Internal(e.to_string()))?;
            if let Some(row) = rows.next().map_err(|e| MoleculerError::Internal(e.to_string()))? {
                Some(row.get(0).map_err(|e| MoleculerError::Internal(e.to_string()))?)
            } else {
                None
            }
        };

        if let Some(blob) = blob_opt {
            let mut job = Self::load_job_from_blob(&blob)?;
            job.status     = JobStatus::Active;
            job.started_at = Some(now);
            job.updated_at = now;
            self.save_job(&job)?;
            Ok(Some(job))
        } else {
            Ok(None)
        }
    }

    pub fn complete_job(&self, workflow: &str, id: &str, result: Value) -> Result<()> {
        if let Some(mut job) = self.get_job(workflow, id)? {
            let now = Utc::now().timestamp_millis();
            job.status      = JobStatus::Completed;
            job.finished_at = Some(now);
            job.updated_at  = now;
            job.result      = Some(result);
            self.save_job(&job)
        } else {
            Err(MoleculerError::NotFound(format!("Job {id} not found")))
        }
    }

    pub fn fail_job(&self, workflow: &str, id: &str, err_msg: &str) -> Result<()> {
        if let Some(mut job) = self.get_job(workflow, id)? {
            let now = Utc::now().timestamp_millis();
            job.retries   += 1;
            job.updated_at = now;

            if job.retries <= job.max_retries {
                // Exponential backoff: retry after 2^retries * base ms
                let backoff = (1 << job.retries.min(10)) as i64 * 1000;
                job.status     = JobStatus::Delayed;
                job.promote_at = Some(now + backoff);
                job.error      = Some(err_msg.to_string());
            } else {
                job.status      = JobStatus::Failed;
                job.finished_at = Some(now);
                job.error       = Some(err_msg.to_string());
            }
            self.save_job(&job)
        } else {
            Err(MoleculerError::NotFound(format!("Job {id} not found")))
        }
    }

    pub fn cancel_job(&self, workflow: &str, id: &str) -> Result<bool> {
        if let Some(mut job) = self.get_job(workflow, id)? {
            let now = Utc::now().timestamp_millis();
            match job.status {
                JobStatus::Active | JobStatus::Waiting | JobStatus::Delayed => {
                    job.status      = JobStatus::Failed;
                    job.finished_at = Some(now);
                    job.updated_at  = now;
                    job.error       = Some("Job cancelled".into());
                    self.save_job(&job)?;
                    Ok(true)
                }
                _ => Ok(false),
            }
        } else {
            Ok(false)
        }
    }

    pub fn list_jobs(
        &self,
        workflow: &str,
        status:   Option<&str>,
        limit:    i64,
        offset:   i64,
    ) -> Result<Vec<Job>> {
        let c = self.conn.lock().unwrap();
        let sql = if status.is_some() {
            "SELECT doc FROM jobs WHERE workflow=?1 AND status=?2 ORDER BY json_extract(doc,'$.created_at') DESC LIMIT ?3 OFFSET ?4".to_string()
        } else {
            "SELECT doc FROM jobs WHERE workflow=?1 ORDER BY json_extract(doc,'$.created_at') DESC LIMIT ?2 OFFSET ?3".to_string()
        };

        let blobs: Vec<String> = {
            if let Some(s) = status {
                let mut stmt = c.prepare(&sql).map_err(|e| MoleculerError::Internal(e.to_string()))?;
                let rows = stmt.query_map(params![workflow, s, limit, offset], |r| r.get::<_, String>(0))
                    .map_err(|e| MoleculerError::Internal(e.to_string()))?;
                rows.filter_map(|r| r.ok()).collect()
            } else {
                let mut stmt = c.prepare(&sql).map_err(|e| MoleculerError::Internal(e.to_string()))?;
                let rows = stmt.query_map(params![workflow, limit, offset], |r| r.get::<_, String>(0))
                    .map_err(|e| MoleculerError::Internal(e.to_string()))?;
                rows.filter_map(|r| r.ok()).collect()
            }
        };

        blobs.iter().map(|b| Self::load_job_from_blob(b)).collect()
    }

    /// Trigger a signal (for inter-job communication).
    pub fn trigger_signal(
        &self,
        name:     &str,
        key:      Option<&str>,
        payload:  Option<Value>,
        exp_secs: Option<i64>,
    ) -> Result<()> {
        let sig_id = format!("{}:{}", name, key.unwrap_or("__default__"));
        let blob   = payload.map(|p| serde_json::to_string(&p)).transpose()
            .map_err(|e| MoleculerError::Internal(e.to_string()))?;
        let exp_at = exp_secs.map(|s| Utc::now().timestamp_millis() + s * 1000);
        let c = self.conn.lock().unwrap();
        c.execute(
            "INSERT OR REPLACE INTO signals (id, payload, exp_at) VALUES (?1,?2,?3)",
            params![sig_id, blob, exp_at],
        ).map_err(|e| MoleculerError::Internal(e.to_string()))?;
        Ok(())
    }

    /// Read a signal; returns None if not set or expired.
    pub fn get_signal(&self, name: &str, key: Option<&str>) -> Result<Option<Value>> {
        let sig_id = format!("{}:{}", name, key.unwrap_or("__default__"));
        let now    = Utc::now().timestamp_millis();
        let c = self.conn.lock().unwrap();
        let mut s = c.prepare(
            "SELECT payload, exp_at FROM signals WHERE id=?1"
        ).map_err(|e| MoleculerError::Internal(e.to_string()))?;
        let mut rows = s.query(params![sig_id])
            .map_err(|e| MoleculerError::Internal(e.to_string()))?;
        if let Some(row) = rows.next().map_err(|e| MoleculerError::Internal(e.to_string()))? {
            let payload: Option<String> = row.get(0).ok();
            let exp_at: Option<i64>    = row.get(1).ok().flatten();
            if exp_at.map_or(false, |e| e < now) {
                // Expired
                drop(rows);
                let _ = c.execute("DELETE FROM signals WHERE id=?1", params![sig_id]);
                return Ok(None);
            }
            if let Some(blob) = payload {
                return Ok(serde_json::from_str(&blob).ok());
            }
            return Ok(Some(json!(null)));
        }
        Ok(None)
    }

    pub fn remove_signal(&self, name: &str, key: Option<&str>) -> Result<()> {
        let sig_id = format!("{}:{}", name, key.unwrap_or("__default__"));
        let c = self.conn.lock().unwrap();
        c.execute("DELETE FROM signals WHERE id=?1", params![sig_id])
            .map_err(|e| MoleculerError::Internal(e.to_string()))?;
        Ok(())
    }

    pub fn job_counts(&self, workflow: &str) -> Result<serde_json::Value> {
        let c = self.conn.lock().unwrap();
        let mut s = c.prepare(
            "SELECT status, COUNT(*) FROM jobs WHERE workflow=?1 GROUP BY status"
        ).map_err(|e| MoleculerError::Internal(e.to_string()))?;
        let mut counts = serde_json::Map::new();
        let mut rows = s.query(params![workflow])
            .map_err(|e| MoleculerError::Internal(e.to_string()))?;
        while let Some(row) = rows.next().map_err(|e| MoleculerError::Internal(e.to_string()))? {
            let status: String = row.get(0).unwrap_or_default();
            let count: i64     = row.get(1).unwrap_or(0);
            counts.insert(status, json!(count));
        }
        Ok(json!(counts))
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_get_job() {
        let a = SqliteJobAdapter::memory();
        let job = a.create_job("my-workflow", serde_json::json!({ "x": 1 }), CreateJobOptions::default()).unwrap();
        assert_eq!(job.workflow, "my-workflow");
        assert_eq!(job.status, JobStatus::Waiting);

        let got = a.get_job("my-workflow", &job.id).unwrap().unwrap();
        assert_eq!(got.id, job.id);
    }

    #[test]
    fn test_claim_and_complete() {
        let a = SqliteJobAdapter::memory();
        a.create_job("wf", serde_json::json!({}), CreateJobOptions::default()).unwrap();

        let job = a.claim_next_job("wf").unwrap().unwrap();
        assert_eq!(job.status, JobStatus::Active);

        a.complete_job("wf", &job.id, serde_json::json!({ "ok": true })).unwrap();
        let done = a.get_job("wf", &job.id).unwrap().unwrap();
        assert_eq!(done.status, JobStatus::Completed);
        assert_eq!(done.result.unwrap()["ok"], true);
    }

    #[test]
    fn test_fail_and_retry() {
        let a = SqliteJobAdapter::memory();
        let opts = CreateJobOptions { retries: Some(1), ..Default::default() };
        let job = a.create_job("wf", serde_json::json!({}), opts).unwrap();

        a.claim_next_job("wf").unwrap();
        a.fail_job("wf", &job.id, "timeout").unwrap();
        let j = a.get_job("wf", &job.id).unwrap().unwrap();
        // 1 retry allowed → should be Delayed (not Failed yet)
        assert_eq!(j.status, JobStatus::Delayed);
        assert_eq!(j.retries, 1);

        // promote by setting promote_at in the past
        {
            let c = a.conn.lock().unwrap();
            c.execute("UPDATE jobs SET doc=json_set(doc,'$.promote_at',1) WHERE id=?1",
                rusqlite::params![job.id]).unwrap();
        }
        let claimed = a.claim_next_job("wf").unwrap().unwrap();
        assert_eq!(claimed.status, JobStatus::Active);

        a.fail_job("wf", &job.id, "timeout again").unwrap();
        let final_j = a.get_job("wf", &job.id).unwrap().unwrap();
        // Now should be fully Failed (retries=2 > max_retries=1)
        assert_eq!(final_j.status, JobStatus::Failed);
    }

    #[test]
    fn test_delayed_job() {
        let a = SqliteJobAdapter::memory();
        let opts = CreateJobOptions { delay: Some(99999999), ..Default::default() };
        let job = a.create_job("wf", serde_json::json!({}), opts).unwrap();
        assert_eq!(job.status, JobStatus::Delayed);

        // Should NOT be claimable yet
        let claimed = a.claim_next_job("wf").unwrap();
        assert!(claimed.is_none());
    }

    #[test]
    fn test_cancel_job() {
        let a = SqliteJobAdapter::memory();
        let job = a.create_job("wf", serde_json::json!({}), CreateJobOptions::default()).unwrap();
        assert!(a.cancel_job("wf", &job.id).unwrap());
        let j = a.get_job("wf", &job.id).unwrap().unwrap();
        assert_eq!(j.status, JobStatus::Failed);
    }

    #[test]
    fn test_signals() {
        let a = SqliteJobAdapter::memory();
        a.trigger_signal("my-signal", Some("key1"), Some(serde_json::json!({ "data": 42 })), None).unwrap();
        let val = a.get_signal("my-signal", Some("key1")).unwrap().unwrap();
        assert_eq!(val["data"], 42);

        a.remove_signal("my-signal", Some("key1")).unwrap();
        assert!(a.get_signal("my-signal", Some("key1")).unwrap().is_none());
    }

    #[test]
    fn test_job_counts() {
        let a = SqliteJobAdapter::memory();
        a.create_job("wf", serde_json::json!({}), CreateJobOptions::default()).unwrap();
        a.create_job("wf", serde_json::json!({}), CreateJobOptions::default()).unwrap();
        let job = a.create_job("wf", serde_json::json!({}), CreateJobOptions::default()).unwrap();
        a.claim_next_job("wf").unwrap();
        a.complete_job("wf", &job.id, serde_json::json!(null)).unwrap();

        let counts = a.job_counts("wf").unwrap();
        // At least 2 waiting + 1 completed
        assert!(counts["waiting"].as_i64().unwrap_or(0) >= 1);
    }
}
