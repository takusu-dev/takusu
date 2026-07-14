use sha2::{Digest, Sha256};

use crate::token_cache::{TokenCache, TokenState};

pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let result = hasher.finalize();
    result.iter().map(|b| format!("{b:02x}")).collect()
}

pub async fn verify_token_with_cache(
    token: &str,
    root_token: &str,
    storage: &dyn takusu_storage::Storage,
    token_cache: &TokenCache,
) -> Result<bool, takusu_storage::StorageError> {
    if token == root_token {
        return Ok(true);
    }

    match token_cache.get(token) {
        Some(TokenState::Valid) => return Ok(true),
        Some(TokenState::Invalid) => return Ok(false),
        None => {}
    }

    let valid = storage.verify_token(token).await?;

    if valid {
        token_cache.put(token, TokenState::Valid);
    } else {
        token_cache.put(token, TokenState::Invalid);
    }

    Ok(valid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use takusu_storage::Storage;
    use takusu_storage::{
        CreateHabit, CreateHabitPause, CreateTask, GoogleCalEventRow, GoogleCalSettingsRow,
        HabitPauseRow, HabitRow, SaveScheduleRequest, ScheduleRow, SettingsRow, TaskQuery, TaskRow,
        TokenCreateResponse, TokenRow, UpdateGoogleCalSettings, UpdateHabit, UpdateSettings,
        UpdateTask,
    };

    // hash_token should be deterministic and 64 hex chars (SHA-256).
    #[test]
    fn hash_token_is_deterministic_64_hex() {
        let h1 = hash_token("tsk_abc");
        let h2 = hash_token("tsk_abc");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
        assert!(h1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_token_different_inputs_differ() {
        assert_ne!(hash_token("a"), hash_token("b"));
        // Empty string still hashes (no panic).
        assert_eq!(hash_token("").len(), 64);
    }

    // Minimal mock storage that only implements verify_token; every other
    // method returns Internal. Counts verify_token calls.
    struct MockStorage {
        valid_tokens: std::collections::HashSet<String>,
        call_count: Arc<AtomicUsize>,
    }

    impl MockStorage {
        fn new(valid: &[&str]) -> (Self, Arc<AtomicUsize>) {
            let count = Arc::new(AtomicUsize::new(0));
            let s = Self {
                valid_tokens: valid.iter().map(|s| s.to_string()).collect(),
                call_count: count.clone(),
            };
            (s, count)
        }
    }

    fn unimplemented<T>() -> Result<T, takusu_storage::StorageError> {
        Err(takusu_storage::StorageError::Internal(
            "mock: not implemented".into(),
        ))
    }

    #[async_trait::async_trait]
    impl Storage for MockStorage {
        async fn verify_token(&self, token: &str) -> Result<bool, takusu_storage::StorageError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(self.valid_tokens.contains(token))
        }
        async fn list_tasks(
            &self,
            _: &TaskQuery,
        ) -> Result<Vec<TaskRow>, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn get_task(&self, _: &str) -> Result<TaskRow, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn create_task(
            &self,
            _: &CreateTask,
        ) -> Result<TaskRow, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn update_task(
            &self,
            _: &str,
            _: &UpdateTask,
        ) -> Result<TaskRow, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn replace_task(
            &self,
            _: &str,
            _: &CreateTask,
        ) -> Result<TaskRow, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn delete_task(&self, _: &str) -> Result<(), takusu_storage::StorageError> {
            unimplemented()
        }
        async fn list_habits(&self) -> Result<Vec<HabitRow>, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn get_habit(&self, _: &str) -> Result<HabitRow, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn create_habit(
            &self,
            _: &CreateHabit,
        ) -> Result<HabitRow, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn update_habit(
            &self,
            _: &str,
            _: &UpdateHabit,
        ) -> Result<HabitRow, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn replace_habit(
            &self,
            _: &str,
            _: &CreateHabit,
        ) -> Result<HabitRow, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn delete_habit(&self, _: &str) -> Result<(), takusu_storage::StorageError> {
            unimplemented()
        }
        async fn list_habit_pauses(
            &self,
            _: &str,
        ) -> Result<Vec<HabitPauseRow>, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn list_all_habit_pauses(
            &self,
        ) -> Result<Vec<HabitPauseRow>, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn create_habit_pause(
            &self,
            _: &str,
            _: &CreateHabitPause,
        ) -> Result<HabitPauseRow, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn delete_habit_pause(
            &self,
            _: &str,
            _: &str,
        ) -> Result<(), takusu_storage::StorageError> {
            unimplemented()
        }
        async fn list_habit_steps(
            &self,
            _: &str,
        ) -> Result<Vec<takusu_storage::HabitStepRow>, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn list_all_habit_steps(
            &self,
        ) -> Result<Vec<takusu_storage::HabitStepRow>, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn replace_habit_steps(
            &self,
            _: &str,
            _: &[takusu_storage::HabitStepInput],
        ) -> Result<Vec<takusu_storage::HabitStepRow>, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn get_schedule(&self) -> Result<Option<ScheduleRow>, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn save_schedule(
            &self,
            _: &SaveScheduleRequest,
        ) -> Result<ScheduleRow, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn clear_schedule(&self) -> Result<(), takusu_storage::StorageError> {
            unimplemented()
        }
        async fn create_token(
            &self,
            _: Option<&str>,
        ) -> Result<TokenCreateResponse, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn list_tokens(&self) -> Result<Vec<TokenRow>, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn revoke_token(&self, _: i64) -> Result<(), takusu_storage::StorageError> {
            unimplemented()
        }
        async fn get_settings(&self) -> Result<SettingsRow, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn update_settings(
            &self,
            _: &UpdateSettings,
        ) -> Result<SettingsRow, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn get_gcal_settings(
            &self,
        ) -> Result<GoogleCalSettingsRow, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn update_gcal_settings(
            &self,
            _: &UpdateGoogleCalSettings,
        ) -> Result<GoogleCalSettingsRow, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn list_gcal_mappings(
            &self,
        ) -> Result<Vec<GoogleCalEventRow>, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn upsert_gcal_mappings(
            &self,
            _: &[(String, String)],
        ) -> Result<(), takusu_storage::StorageError> {
            unimplemented()
        }
        async fn delete_gcal_mappings(
            &self,
            _: &[String],
        ) -> Result<(), takusu_storage::StorageError> {
            unimplemented()
        }
        async fn clear_gcal_mappings(&self) -> Result<(), takusu_storage::StorageError> {
            unimplemented()
        }
        async fn health_check(&self) -> Result<String, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn list_skills(
            &self,
        ) -> Result<Vec<takusu_storage::SkillRow>, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn get_skill(
            &self,
            _: &str,
        ) -> Result<takusu_storage::SkillRow, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn create_skill(
            &self,
            _: &takusu_storage::CreateSkill,
        ) -> Result<takusu_storage::SkillRow, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn update_skill(
            &self,
            _: &str,
            _: &takusu_storage::UpdateSkill,
        ) -> Result<takusu_storage::SkillRow, takusu_storage::StorageError> {
            unimplemented()
        }
        async fn delete_skill(&self, _: &str) -> Result<(), takusu_storage::StorageError> {
            unimplemented()
        }
    }

    #[tokio::test]
    async fn verify_token_with_cache_root_token_short_circuits() {
        let (storage, calls) = MockStorage::new(&["real-token"]);
        let cache = TokenCache::new(std::time::Duration::from_secs(60));
        // Root token matches → true without touching storage.
        let ok = verify_token_with_cache("root", "root", &storage, &cache)
            .await
            .unwrap();
        assert!(ok);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn verify_token_with_cache_caches_valid_answer() {
        let (storage, calls) = MockStorage::new(&["real-token"]);
        let cache = TokenCache::new(std::time::Duration::from_secs(60));
        let ok1 = verify_token_with_cache("real-token", "root", &storage, &cache)
            .await
            .unwrap();
        assert!(ok1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // Second call should hit the cache, not storage.
        let ok2 = verify_token_with_cache("real-token", "root", &storage, &cache)
            .await
            .unwrap();
        assert!(ok2);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn verify_token_with_cache_caches_invalid_answer() {
        let (storage, calls) = MockStorage::new(&[]);
        let cache = TokenCache::new(std::time::Duration::from_secs(60));
        let ok1 = verify_token_with_cache("bogus", "root", &storage, &cache)
            .await
            .unwrap();
        assert!(!ok1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // Cached Invalid → no second storage hit.
        let ok2 = verify_token_with_cache("bogus", "root", &storage, &cache)
            .await
            .unwrap();
        assert!(!ok2);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
