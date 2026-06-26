use thiserror::Error;

pub const MAX_TITLE_LEN: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TaskError {
    #[error("task title must not be empty")]
    EmptyTitle,
    #[error("task title too long")]
    TitleTooLong,
    #[error("requirement id must not be empty")]
    EmptyRequirement,
    #[error("acceptance criterion must not be empty")]
    EmptyCriterion,
    #[error("unknown dependency: {0}")]
    UnknownDependency(String),
    #[error("dependencies not satisfied")]
    DependenciesNotSatisfied,
    #[error("transition not allowed: {from} -> {to}")]
    TransitionNotAllowed { from: &'static str, to: &'static str },
    #[error("no such task: {0}")]
    NoSuchTask(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Dispatched,
    Running,
    Delivered,
    Verified,
    Failed,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Dispatched => "DISPATCHED",
            Self::Running => "RUNNING",
            Self::Delivered => "DELIVERED",
            Self::Verified => "VERIFIED",
            Self::Failed => "FAILED",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "PENDING" => Some(Self::Pending),
            "DISPATCHED" => Some(Self::Dispatched),
            "RUNNING" => Some(Self::Running),
            "DELIVERED" => Some(Self::Delivered),
            "VERIFIED" => Some(Self::Verified),
            "FAILED" => Some(Self::Failed),
            _ => None,
        }
    }

    pub fn can_transition_to(self, to: TaskStatus) -> bool {
        use TaskStatus::*;
        matches!(
            (self, to),
            (Pending, Dispatched)
                | (Dispatched, Running)
                | (Dispatched, Failed)
                | (Running, Delivered)
                | (Running, Failed)
                | (Delivered, Verified)
                | (Delivered, Failed)
                | (Failed, Pending)
        )
    }
}

fn validate_title(title: &str) -> Result<String, TaskError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(TaskError::EmptyTitle);
    }
    if title.chars().count() > MAX_TITLE_LEN {
        return Err(TaskError::TitleTooLong);
    }
    Ok(title.to_string())
}

fn validate_criteria(raw: &[String]) -> Result<Vec<String>, TaskError> {
    raw.iter()
        .map(|c| {
            let c = c.trim();
            if c.is_empty() {
                Err(TaskError::EmptyCriterion)
            } else {
                Ok(c.to_string())
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTask {
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Vec<String>,
    pub dependencies: Vec<String>,
    pub points: i32,
}

impl NewTask {
    pub fn new(
        title: &str,
        description: &str,
        acceptance_criteria: &[String],
        dependencies: &[String],
    ) -> Result<Self, TaskError> {
        Ok(Self {
            title: validate_title(title)?,
            description: description.trim().to_string(),
            acceptance_criteria: validate_criteria(acceptance_criteria)?,
            dependencies: dependencies.to_vec(),
            points: 0,
        })
    }

    pub fn with_points(mut self, points: i32) -> Self {
        self.points = points.max(0);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Vec<String>,
    pub dependencies: Vec<String>,
    pub status: TaskStatus,
    pub points: i32,
    pub assignee: String,
    pub assignee_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decomposition {
    pub id: String,
    pub requirement_id: String,
    pub requirement_version: u32,
    pub tasks: Vec<Task>,
}

impl Decomposition {
    pub fn new(id: &str, requirement_id: &str, requirement_version: u32) -> Self {
        Self {
            id: id.to_string(),
            requirement_id: requirement_id.to_string(),
            requirement_version,
            tasks: Vec::new(),
        }
    }

    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    fn task_mut(&mut self, id: &str) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    /// Deps must point at already-existing tasks: forces topological insertion, so the DAG can't cycle.
    pub fn add_task(&mut self, new: NewTask) -> Result<String, TaskError> {
        for dep in &new.dependencies {
            if self.task(dep).is_none() {
                return Err(TaskError::UnknownDependency(dep.clone()));
            }
        }
        let id = format!("t{}", self.tasks.len() + 1);
        let mut deps = Vec::new();
        for d in new.dependencies {
            if !deps.contains(&d) {
                deps.push(d);
            }
        }
        self.tasks.push(Task {
            id: id.clone(),
            title: new.title,
            description: new.description,
            acceptance_criteria: new.acceptance_criteria,
            dependencies: deps,
            status: TaskStatus::Pending,
            points: new.points,
            assignee: String::new(),
            assignee_kind: String::new(),
        });
        Ok(id)
    }

    pub fn set_points(&mut self, id: &str, points: i32) -> Result<(), TaskError> {
        let task = self.task_mut(id).ok_or_else(|| TaskError::NoSuchTask(id.to_string()))?;
        task.points = points.max(0);
        Ok(())
    }

    /// Empty assignee cancels the assignment and clears kind too.
    pub fn set_assignee(&mut self, id: &str, assignee: &str, kind: &str) -> Result<(), TaskError> {
        let task = self.task_mut(id).ok_or_else(|| TaskError::NoSuchTask(id.to_string()))?;
        let a = assignee.trim();
        task.assignee = a.to_string();
        task.assignee_kind = if a.is_empty() { String::new() } else { kind.trim().to_string() };
        Ok(())
    }

    /// A task's deps are satisfied only when every dependency is Verified.
    pub fn dependencies_satisfied(&self, id: &str) -> bool {
        match self.task(id) {
            Some(t) => t
                .dependencies
                .iter()
                .all(|d| self.task(d).map(|dt| dt.status == TaskStatus::Verified).unwrap_or(false)),
            None => false,
        }
    }

    pub fn ready_tasks(&self) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Pending && self.dependencies_satisfied(&t.id))
            .collect()
    }

    /// An empty decomposition is never complete.
    pub fn is_complete(&self) -> bool {
        !self.tasks.is_empty() && self.tasks.iter().all(|t| t.status == TaskStatus::Verified)
    }

    pub fn dispatch(&mut self, id: &str) -> Result<(), TaskError> {
        if !self.dependencies_satisfied(id) {
            if self.task(id).is_none() {
                return Err(TaskError::NoSuchTask(id.to_string()));
            }
            return Err(TaskError::DependenciesNotSatisfied);
        }
        self.transition(id, TaskStatus::Dispatched)
    }

    /// Idempotent step toward `target` along the happy path; unreachable targets are a no-op, not an error.
    pub fn advance_to(&mut self, id: &str, target: TaskStatus) -> Result<(), TaskError> {
        if self.task(id).is_none() {
            return Err(TaskError::NoSuchTask(id.to_string()));
        }
        loop {
            let cur = self.task(id).expect("exists").status;
            if cur == target {
                return Ok(());
            }
            let Some(next) = next_toward(cur, target) else {
                return Ok(());
            };
            if next == TaskStatus::Dispatched {
                self.dispatch(id)?;
            } else {
                self.transition(id, next)?;
            }
        }
    }

    /// Use `dispatch` rather than this for Pending→Dispatched so the dependency gate applies.
    pub fn transition(&mut self, id: &str, to: TaskStatus) -> Result<(), TaskError> {
        let satisfied = to != TaskStatus::Dispatched || self.dependencies_satisfied(id);
        let task = self.task_mut(id).ok_or_else(|| TaskError::NoSuchTask(id.to_string()))?;
        if !task.status.can_transition_to(to) {
            return Err(TaskError::TransitionNotAllowed { from: task.status.as_str(), to: to.as_str() });
        }
        if !satisfied {
            return Err(TaskError::DependenciesNotSatisfied);
        }
        task.status = to;
        Ok(())
    }
}

fn next_toward(cur: TaskStatus, target: TaskStatus) -> Option<TaskStatus> {
    use TaskStatus::*;
    if target == Failed {
        return match cur {
            Dispatched | Running | Delivered => Some(Failed),
            _ => None,
        };
    }
    fn rank(s: TaskStatus) -> Option<u8> {
        match s {
            Pending => Some(0),
            Dispatched => Some(1),
            Running => Some(2),
            Delivered => Some(3),
            Verified => Some(4),
            Failed => None,
        }
    }
    let (rc, rt) = (rank(cur)?, rank(target)?);
    if rc >= rt {
        return None;
    }
    Some(match rc + 1 {
        1 => Dispatched,
        2 => Running,
        3 => Delivered,
        _ => Verified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nt(title: &str, deps: &[&str]) -> NewTask {
        NewTask::new(title, "", &[], &deps.iter().map(|s| s.to_string()).collect::<Vec<_>>())
            .expect("valid")
    }

    fn drive_to_verified(d: &mut Decomposition, id: &str) {
        d.dispatch(id).expect("dispatch");
        d.transition(id, TaskStatus::Running).expect("running");
        d.transition(id, TaskStatus::Delivered).expect("delivered");
        d.transition(id, TaskStatus::Verified).expect("verified");
    }

    #[test]
    fn new_task_validates_title_and_criteria() {
        assert_eq!(NewTask::new("  ", "", &[], &[]).unwrap_err(), TaskError::EmptyTitle);
        assert_eq!(
            NewTask::new("t", "", &["ok".into(), "  ".into()], &[]).unwrap_err(),
            TaskError::EmptyCriterion
        );
        let t = NewTask::new(" build ", " do it ", &[" c1 ".into()], &[]).expect("ok");
        assert_eq!(t.title, "build");
        assert_eq!(t.description, "do it");
        assert_eq!(t.acceptance_criteria, vec!["c1".to_string()]);
    }

    #[test]
    fn add_task_assigns_sequential_local_ids() {
        let mut d = Decomposition::new("d1", "req1", 1);
        assert_eq!(d.add_task(nt("A", &[])).expect("a"), "t1");
        assert_eq!(d.add_task(nt("B", &[])).expect("b"), "t2");
        assert_eq!(d.tasks.len(), 2);
        assert!(d.tasks.iter().all(|t| t.status == TaskStatus::Pending));
    }

    #[test]
    fn dependency_must_reference_existing_task() {
        let mut d = Decomposition::new("d1", "req1", 1);
        assert_eq!(
            d.add_task(nt("B", &["t1"])).unwrap_err(),
            TaskError::UnknownDependency("t1".into())
        );
        d.add_task(nt("A", &[])).expect("a");
        assert!(d.add_task(nt("B", &["t1"])).is_ok());
    }

    #[test]
    fn dependencies_deduped_preserving_order() {
        let mut d = Decomposition::new("d1", "req1", 1);
        d.add_task(nt("A", &[])).expect("a");
        let b = d.add_task(nt("B", &["t1", "t1"])).expect("b");
        assert_eq!(d.task(&b).expect("b").dependencies, vec!["t1".to_string()]);
    }

    #[test]
    fn ready_tasks_are_pending_with_all_deps_verified() {
        let mut d = Decomposition::new("d1", "req1", 1);
        d.add_task(nt("A", &[])).expect("a");
        d.add_task(nt("B", &["t1"])).expect("b");
        let ready: Vec<_> = d.ready_tasks().iter().map(|t| t.id.clone()).collect();
        assert_eq!(ready, vec!["t1".to_string()]);

        assert_eq!(d.dispatch("t2").unwrap_err(), TaskError::DependenciesNotSatisfied);

        drive_to_verified(&mut d, "t1");
        let ready: Vec<_> = d.ready_tasks().iter().map(|t| t.id.clone()).collect();
        assert_eq!(ready, vec!["t2".to_string()]);
        assert!(d.dispatch("t2").is_ok());
    }

    #[test]
    fn independent_delivery_diamond_dag() {
        let mut d = Decomposition::new("d1", "req1", 1);
        d.add_task(nt("A", &[])).expect("a");
        d.add_task(nt("B", &["t1"])).expect("b");
        d.add_task(nt("C", &["t1"])).expect("c");
        d.add_task(nt("D", &["t2", "t3"])).expect("d");

        drive_to_verified(&mut d, "t1");
        let ready: Vec<_> = d.ready_tasks().iter().map(|t| t.id.clone()).collect();
        assert_eq!(ready, vec!["t2".to_string(), "t3".to_string()]);
        assert_eq!(d.dispatch("t4").unwrap_err(), TaskError::DependenciesNotSatisfied);

        drive_to_verified(&mut d, "t2");
        assert_eq!(d.dispatch("t4").unwrap_err(), TaskError::DependenciesNotSatisfied);
        drive_to_verified(&mut d, "t3");
        assert!(d.dispatch("t4").is_ok());
        assert!(!d.is_complete());
        d.transition("t4", TaskStatus::Running).expect("r");
        d.transition("t4", TaskStatus::Delivered).expect("d");
        d.transition("t4", TaskStatus::Verified).expect("v");
        assert!(d.is_complete());
    }

    #[test]
    fn advance_to_walks_happy_path_for_root_task() {
        let mut d = Decomposition::new("d1", "req1", 1);
        d.add_task(nt("A", &[])).expect("a");
        d.advance_to("t1", TaskStatus::Delivered).expect("advance");
        assert_eq!(d.task("t1").expect("t1").status, TaskStatus::Delivered);
    }

    #[test]
    fn advance_to_is_idempotent_when_already_past() {
        let mut d = Decomposition::new("d1", "req1", 1);
        d.add_task(nt("A", &[])).expect("a");
        d.advance_to("t1", TaskStatus::Delivered).expect("to delivered");
        d.advance_to("t1", TaskStatus::Running).expect("noop");
        assert_eq!(d.task("t1").expect("t1").status, TaskStatus::Delivered);
    }

    #[test]
    fn advance_to_verified_unlocks_dependents() {
        let mut d = Decomposition::new("d1", "req1", 1);
        d.add_task(nt("A", &[])).expect("a");
        d.add_task(nt("B", &["t1"])).expect("b");
        d.advance_to("t1", TaskStatus::Verified).expect("verify A");
        assert_eq!(d.task("t1").expect("t1").status, TaskStatus::Verified);
        assert_eq!(d.ready_tasks().iter().map(|t| t.id.clone()).collect::<Vec<_>>(), vec!["t2"]);
        d.advance_to("t2", TaskStatus::Verified).expect("verify B");
        assert!(d.is_complete());
    }

    #[test]
    fn advance_to_failed_from_running() {
        let mut d = Decomposition::new("d1", "req1", 1);
        d.add_task(nt("A", &[])).expect("a");
        d.advance_to("t1", TaskStatus::Running).expect("to running");
        d.advance_to("t1", TaskStatus::Failed).expect("to failed");
        assert_eq!(d.task("t1").expect("t1").status, TaskStatus::Failed);
    }

    #[test]
    fn advance_to_blocked_by_unsatisfied_deps() {
        let mut d = Decomposition::new("d1", "req1", 1);
        d.add_task(nt("A", &[])).expect("a");
        d.add_task(nt("B", &["t1"])).expect("b");
        assert_eq!(
            d.advance_to("t2", TaskStatus::Running).unwrap_err(),
            TaskError::DependenciesNotSatisfied
        );
        assert_eq!(d.task("t2").expect("t2").status, TaskStatus::Pending);
    }

    #[test]
    fn illegal_transition_rejected() {
        let mut d = Decomposition::new("d1", "req1", 1);
        d.add_task(nt("A", &[])).expect("a");
        assert_eq!(
            d.transition("t1", TaskStatus::Running).unwrap_err(),
            TaskError::TransitionNotAllowed { from: "PENDING", to: "RUNNING" }
        );
    }

    #[test]
    fn failed_can_retry_back_to_pending() {
        let mut d = Decomposition::new("d1", "req1", 1);
        d.add_task(nt("A", &[])).expect("a");
        d.dispatch("t1").expect("dispatch");
        d.transition("t1", TaskStatus::Running).expect("run");
        d.transition("t1", TaskStatus::Failed).expect("fail");
        d.transition("t1", TaskStatus::Pending).expect("retry");
        assert_eq!(d.task("t1").expect("t1").status, TaskStatus::Pending);
    }

    #[test]
    fn transition_unknown_task_errors() {
        let mut d = Decomposition::new("d1", "req1", 1);
        assert_eq!(
            d.transition("ghost", TaskStatus::Running).unwrap_err(),
            TaskError::NoSuchTask("ghost".into())
        );
        assert_eq!(d.dispatch("ghost").unwrap_err(), TaskError::NoSuchTask("ghost".into()));
    }

    #[test]
    fn status_str_roundtrip() {
        for s in [
            TaskStatus::Pending,
            TaskStatus::Dispatched,
            TaskStatus::Running,
            TaskStatus::Delivered,
            TaskStatus::Verified,
            TaskStatus::Failed,
        ] {
            assert_eq!(TaskStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(TaskStatus::parse("NOPE"), None);
    }
}
