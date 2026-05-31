//! 组织 CRUD 用例。名称唯一性忽略软删除;列表复用 kernel 分页。

use std::sync::Arc;

use kernel::page::{Page, PageRequest};
use thiserror::Error;

use crate::domain::{NewOrganization, OrgError, Organization};
use crate::ports::{OrgRepoError, OrgRepository};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrgCmdError {
    #[error(transparent)]
    Validation(#[from] OrgError),
    #[error("organization name already exists")]
    NameExists,
    #[error("organization not found")]
    NotFound,
    #[error(transparent)]
    Repo(#[from] OrgRepoError),
}

#[derive(Clone)]
pub struct OrganizationService {
    repo: Arc<dyn OrgRepository>,
}

impl OrganizationService {
    pub fn new(repo: Arc<dyn OrgRepository>) -> Self {
        Self { repo }
    }

    pub async fn create(&self, name: &str) -> Result<Organization, OrgCmdError> {
        let new_org = NewOrganization::new(name)?;
        if self.repo.find_active_by_name(&new_org.name).await?.is_some() {
            return Err(OrgCmdError::NameExists);
        }
        Ok(self.repo.insert(&new_org).await?)
    }

    pub async fn list(&self, page: PageRequest) -> Result<Page<Organization>, OrgCmdError> {
        let total = self.repo.count_active().await?;
        let items = self.repo.list_active(page.offset(), page.page_size()).await?;
        Ok(Page::of(page, total, items))
    }

    pub async fn get(&self, id: &str) -> Result<Organization, OrgCmdError> {
        self.repo.get(id).await?.filter(|o| !o.deleted).ok_or(OrgCmdError::NotFound)
    }

    /// 改名 + 启停。改名时仍要保证未删除组织内唯一。
    pub async fn update(
        &self,
        id: &str,
        name: &str,
        enable: bool,
    ) -> Result<Organization, OrgCmdError> {
        let mut org = self.get(id).await?;
        if name.trim() != org.name {
            if let Some(existing) = self.repo.find_active_by_name(name.trim()).await? {
                if existing.id != id {
                    return Err(OrgCmdError::NameExists);
                }
            }
            org.rename(name)?;
        }
        org.set_enabled(enable);
        self.repo.save(&org).await?;
        Ok(org)
    }

    pub async fn delete(&self, id: &str) -> Result<(), OrgCmdError> {
        let mut org = self.get(id).await?;
        org.soft_delete();
        self.repo.save(&org).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::InMemoryOrgRepository;

    fn svc() -> OrganizationService {
        OrganizationService::new(Arc::new(InMemoryOrgRepository::new()))
    }

    #[tokio::test]
    async fn create_then_get() {
        let s = svc();
        let o = s.create("Acme").await.expect("create");
        assert_eq!(s.get(&o.id).await.expect("get").name, "Acme");
    }

    #[tokio::test]
    async fn duplicate_name_rejected_but_allowed_after_delete() {
        let s = svc();
        let o = s.create("Acme").await.expect("ok");
        assert_eq!(s.create("Acme").await.unwrap_err(), OrgCmdError::NameExists);
        s.delete(&o.id).await.expect("del");
        assert!(s.create("Acme").await.is_ok()); // 软删除后名字释放
    }

    #[tokio::test]
    async fn list_paginated() {
        let s = svc();
        for i in 1..=5 {
            s.create(&format!("Org{i}")).await.expect("seed");
        }
        let page = s.list(PageRequest::new(1, 2).expect("v")).await.expect("list");
        assert_eq!(page.total, 5);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.total_pages(), 3);
    }

    #[tokio::test]
    async fn update_rename_and_disable() {
        let s = svc();
        let o = s.create("Acme").await.expect("ok");
        let up = s.update(&o.id, "AcmeCorp", false).await.expect("update");
        assert_eq!(up.name, "AcmeCorp");
        assert!(!up.enable);
    }

    #[tokio::test]
    async fn update_to_taken_name_rejected() {
        let s = svc();
        s.create("A").await.expect("ok");
        let b = s.create("B").await.expect("ok");
        assert_eq!(s.update(&b.id, "A", true).await.unwrap_err(), OrgCmdError::NameExists);
    }

    #[tokio::test]
    async fn get_missing_or_deleted_is_not_found() {
        let s = svc();
        assert_eq!(s.get("ghost").await.unwrap_err(), OrgCmdError::NotFound);
        let o = s.create("X").await.expect("ok");
        s.delete(&o.id).await.expect("del");
        assert_eq!(s.get(&o.id).await.unwrap_err(), OrgCmdError::NotFound);
    }
}
