use std::sync::Mutex;

use async_trait::async_trait;

use crate::domain::{NewMember, ProjectMember};
use crate::ports::{ProjectMemberRepository, RepoError};

#[derive(Default)]
pub struct InMemoryProjectMemberRepository {
    rows: Mutex<Vec<ProjectMember>>,
}

impl InMemoryProjectMemberRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ProjectMemberRepository for InMemoryProjectMemberRepository {
    async fn upsert(&self, m: &NewMember) -> Result<ProjectMember, RepoError> {
        let mut rows = self.rows.lock().expect("lock");
        let out = ProjectMember {
            project_id: m.project_id.clone(),
            user_id: m.user_id.clone(),
            role: m.role,
            added_at: String::new(),
        };
        match rows.iter_mut().find(|r| r.project_id == m.project_id && r.user_id == m.user_id) {
            Some(r) => r.role = m.role, // 改角色,保留加入时间
            None => rows.push(out.clone()),
        }
        // 返回当前态(改角色时 added_at 取已存在行)。
        let cur = rows
            .iter()
            .find(|r| r.project_id == m.project_id && r.user_id == m.user_id)
            .cloned()
            .expect("just inserted");
        Ok(cur)
    }

    async fn list(&self, project_id: &str) -> Result<Vec<ProjectMember>, RepoError> {
        let rows = self.rows.lock().expect("lock");
        Ok(rows.iter().filter(|r| r.project_id == project_id).cloned().collect())
    }

    async fn remove(&self, project_id: &str, user_id: &str) -> Result<bool, RepoError> {
        let mut rows = self.rows.lock().expect("lock");
        let before = rows.len();
        rows.retain(|r| !(r.project_id == project_id && r.user_id == user_id));
        Ok(rows.len() != before)
    }
}
