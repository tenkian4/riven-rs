use super::*;

impl JobQueue {
    pub async fn schedule_index_at(&self, job: IndexJob, run_at: DateTime<Utc>) {
        if run_at <= Utc::now() {
            self.clear_scheduled_index(job.id).await;
            self.push_index(job).await;
            return;
        }
        let id = job.id;
        let task_id = self.pick_scheduled_index_task_id(id).await;
        self.schedule_apalis_task("index", id, &self.index_storage, task_id, job, run_at)
            .await;
    }

    pub async fn clear_scheduled_index(&self, id: i64) {
        for task_id in [scheduled_index_task_id(id), scheduled_index_alt_task_id(id)] {
            self.clear_apalis_scheduled_task(
                "index",
                id,
                self.index_storage.get_config(),
                &task_id.to_string(),
            )
            .await;
        }
    }

    /// Choose which of the item's two task ids to schedule under: one that is
    /// already scheduled (so "earliest wins" still applies), otherwise the
    /// primary unless the primary is live — job data present but not
    /// scheduled means a worker is running it right now, and overwriting it
    /// would orphan the new schedule when that run is acknowledged.
    async fn pick_scheduled_index_task_id(&self, id: i64) -> Ulid {
        let primary = scheduled_index_task_id(id);
        let alt = scheduled_index_alt_task_id(id);
        let config = self.index_storage.get_config();
        let mut conn = self.redis.clone();
        for candidate in [primary, alt] {
            let scheduled: Option<i64> = redis::cmd("ZSCORE")
                .arg(config.scheduled_jobs_set())
                .arg(candidate.to_string())
                .query_async(&mut conn)
                .await
                .ok()
                .flatten();
            if scheduled.is_some() {
                return candidate;
            }
        }
        let primary_live: bool = redis::cmd("HEXISTS")
            .arg(config.job_data_hash())
            .arg(primary.to_string())
            .query_async(&mut conn)
            .await
            .unwrap_or(false);
        if primary_live { alt } else { primary }
    }

    /// Force-overwrite a scheduled task: clear any prior entry for this
    /// deterministic `task_id` (data/meta/scheduled/done/dead/active),
    /// then push the fresh payload through apalis's own `push_task` so the
    /// wire format (metadata hash, scheduled-set entry) is always whatever the
    /// installed apalis-redis version actually expects — no hand-rolled copy
    /// of its schema to drift out of sync. The deterministic task_id per item
    /// gives us "latest call wins" semantics; the clear step is required
    /// because apalis's own push is insert-if-absent (`HSETNX`) and won't
    /// overwrite a still-present entry on its own.
    async fn schedule_apalis_task<Args>(
        &self,
        kind: &'static str,
        id: i64,
        storage: &RedisStorage<Args>,
        task_id: Ulid,
        job: Args,
        run_at: DateTime<Utc>,
    ) where
        Args: Serialize + DeserializeOwned + Unpin + Send + Sync + 'static,
    {
        let config = storage.get_config();
        let mut conn = self.redis.clone();

        let existing: Option<i64> = redis::cmd("ZSCORE")
            .arg(config.scheduled_jobs_set())
            .arg(task_id.to_string())
            .query_async(&mut conn)
            .await
            .ok()
            .flatten();
        if let Some(existing_ts) = existing
            && existing_ts <= run_at.timestamp()
        {
            tracing::debug!(
                id,
                kind,
                existing_run_at = existing_ts,
                requested_run_at = run_at.timestamp(),
                "scheduled task already pending earlier; keeping existing schedule"
            );
            return;
        }

        let task_id_str = task_id.to_string();
        let meta_key = format!("{}:{}", config.job_meta_hash(), task_id_str);
        let clear_result: redis::RedisResult<()> = redis::pipe()
            .atomic()
            .hdel(config.job_data_hash(), &task_id_str)
            .del(&meta_key)
            .zrem(config.scheduled_jobs_set(), &task_id_str)
            .srem(config.done_jobs_set(), &task_id_str)
            .srem(config.dead_jobs_set(), &task_id_str)
            .lrem(config.active_jobs_list(), 0, &task_id_str)
            .query_async(&mut conn)
            .await;
        if let Err(error) = clear_result {
            tracing::error!(id, kind, %error, "failed to clear prior scheduled task state");
            return;
        }

        let task = TaskBuilder::new(job)
            .task_id(TaskId::from_ulid(task_id))
            .run_at_timestamp(run_at.timestamp().max(0).cast_unsigned())
            .build();
        match storage.clone().push_task(task).await {
            Ok(()) => tracing::debug!(id, kind, run_at = %run_at, "scheduled delayed job"),
            Err(error) => tracing::error!(id, kind, %error, "failed to schedule delayed job"),
        }
    }

    async fn clear_apalis_scheduled_task(
        &self,
        kind: &'static str,
        id: i64,
        config: &Config,
        task_id: &str,
    ) {
        let meta_key = format!("{}:{}", config.job_meta_hash(), task_id);
        let mut conn = self.redis.clone();
        let result: redis::RedisResult<()> = redis::pipe()
            .atomic()
            .zrem(config.scheduled_jobs_set(), task_id)
            .hdel(config.job_data_hash(), task_id)
            .del(&meta_key)
            .query_async(&mut conn)
            .await;
        if let Err(error) = result {
            tracing::error!(id, kind, %error, "failed to clear scheduled job");
        }
    }
}
