use std::collections::HashSet;

use thiserror::Error;

pub const MAX_NAME_LEN: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SkillError {
    #[error("skill name must not be empty")]
    EmptyName,
    #[error("skill name too long")]
    NameTooLong,
    #[error("skill instructions must not be empty")]
    EmptyInstructions,
    #[error("unknown included skill: {0}")]
    UnknownInclude(String),
    #[error("cyclic skill composition at: {0}")]
    Cycle(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSkill {
    pub project_id: String,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub includes: Vec<String>,
}

impl NewSkill {
    pub fn new(
        project_id: &str,
        name: &str,
        description: &str,
        instructions: &str,
        includes: &[String],
    ) -> Result<Self, SkillError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(SkillError::EmptyName);
        }
        if name.chars().count() > MAX_NAME_LEN {
            return Err(SkillError::NameTooLong);
        }
        let instructions = instructions.trim();
        if instructions.is_empty() {
            return Err(SkillError::EmptyInstructions);
        }
        Ok(Self {
            project_id: project_id.trim().to_string(),
            name: name.to_string(),
            description: description.trim().to_string(),
            instructions: instructions.to_string(),
            includes: includes.to_vec(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skill {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub includes: Vec<String>,
    pub enabled: bool,
    pub deleted: bool,
}

impl Skill {
    pub fn from_new(id: &str, new: &NewSkill) -> Self {
        Self {
            id: id.to_string(),
            project_id: new.project_id.clone(),
            name: new.name.clone(),
            description: new.description.clone(),
            instructions: new.instructions.clone(),
            includes: new.includes.clone(),
            enabled: true,
            deleted: false,
        }
    }

    pub fn rename(&mut self, name: &str) -> Result<(), SkillError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(SkillError::EmptyName);
        }
        if name.chars().count() > MAX_NAME_LEN {
            return Err(SkillError::NameTooLong);
        }
        self.name = name.to_string();
        Ok(())
    }

    pub fn set_instructions(&mut self, instructions: &str) -> Result<(), SkillError> {
        let instructions = instructions.trim();
        if instructions.is_empty() {
            return Err(SkillError::EmptyInstructions);
        }
        self.instructions = instructions.to_string();
        Ok(())
    }

    pub fn set_description(&mut self, description: &str) {
        self.description = description.trim().to_string();
    }

    pub fn set_includes(&mut self, includes: Vec<String>) {
        self.includes = includes;
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
    }

    pub fn soft_delete(&mut self) {
        self.deleted = true;
    }

    pub fn occupies_name(&self) -> bool {
        !self.deleted
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Composition {
    pub skill_ids: Vec<String>,
    pub instructions: String,
}

pub struct SkillLibrary {
    skills: Vec<Skill>,
}

impl SkillLibrary {
    pub fn new(skills: Vec<Skill>) -> Self {
        Self { skills }
    }

    pub fn get(&self, id: &str) -> Option<&Skill> {
        self.skills.iter().find(|s| s.id == id)
    }

    pub fn compose(&self, roots: &[String]) -> Result<Composition, SkillError> {
        let mut order: Vec<String> = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut on_path: Vec<String> = Vec::new();
        for r in roots {
            self.dfs(r, &mut order, &mut visited, &mut on_path)?;
        }
        let instructions = order
            .iter()
            .map(|id| {
                let s = self.get(id).expect("resolved id exists");
                format!("## {}\n{}", s.name, s.instructions)
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        Ok(Composition { skill_ids: order, instructions })
    }

    fn dfs(
        &self,
        id: &str,
        order: &mut Vec<String>,
        visited: &mut HashSet<String>,
        on_path: &mut Vec<String>,
    ) -> Result<(), SkillError> {
        if visited.contains(id) {
            return Ok(());
        }
        if on_path.iter().any(|p| p == id) {
            return Err(SkillError::Cycle(id.to_string()));
        }
        let skill = self.get(id).ok_or_else(|| SkillError::UnknownInclude(id.to_string()))?;
        on_path.push(id.to_string());
        for inc in &skill.includes {
            self.dfs(inc, order, visited, on_path)?;
        }
        on_path.pop();
        visited.insert(id.to_string());
        order.push(id.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(id: &str, name: &str, includes: &[&str]) -> Skill {
        Skill {
            id: id.into(),
            project_id: "p1".into(),
            name: name.into(),
            description: String::new(),
            instructions: format!("do {name}"),
            includes: includes.iter().map(|s| s.to_string()).collect(),
            enabled: true,
            deleted: false,
        }
    }

    #[test]
    fn new_skill_validates() {
        assert_eq!(NewSkill::new("p", " ", "", "x", &[]).unwrap_err(), SkillError::EmptyName);
        assert_eq!(
            NewSkill::new("p", "n", "", "  ", &[]).unwrap_err(),
            SkillError::EmptyInstructions
        );
        let s = NewSkill::new("p", " Rust 规范 ", "", " 用 thiserror ", &[]).expect("ok");
        assert_eq!(s.name, "Rust 规范");
        assert_eq!(s.instructions, "用 thiserror");
    }

    #[test]
    fn compose_single() {
        let lib = SkillLibrary::new(vec![skill("s1", "A", &[])]);
        let c = lib.compose(&["s1".into()]).expect("compose");
        assert_eq!(c.skill_ids, vec!["s1"]);
        assert_eq!(c.instructions, "## A\ndo A");
    }

    #[test]
    fn compose_expands_includes_dependencies_first() {
        let lib = SkillLibrary::new(vec![skill("s1", "A", &[]), skill("s2", "B", &["s1"])]);
        let c = lib.compose(&["s2".into()]).expect("compose");
        assert_eq!(c.skill_ids, vec!["s1", "s2"]);
        assert!(c.instructions.starts_with("## A\ndo A"));
        assert!(c.instructions.contains("## B\ndo B"));
    }

    #[test]
    fn compose_dedups_diamond() {
        let lib = SkillLibrary::new(vec![
            skill("a", "A", &[]),
            skill("b", "B", &["a"]),
            skill("c", "C", &["a"]),
            skill("d", "D", &["b", "c"]),
        ]);
        let c = lib.compose(&["d".into()]).expect("compose");
        assert_eq!(c.skill_ids, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn compose_detects_cycle() {
        let lib = SkillLibrary::new(vec![skill("a", "A", &["b"]), skill("b", "B", &["a"])]);
        assert_eq!(lib.compose(&["a".into()]).unwrap_err(), SkillError::Cycle("a".into()));
    }

    #[test]
    fn compose_unknown_include_errors() {
        let lib = SkillLibrary::new(vec![skill("a", "A", &["ghost"])]);
        assert_eq!(
            lib.compose(&["a".into()]).unwrap_err(),
            SkillError::UnknownInclude("ghost".into())
        );
        assert_eq!(
            lib.compose(&["nope".into()]).unwrap_err(),
            SkillError::UnknownInclude("nope".into())
        );
    }

    #[test]
    fn compose_multiple_roots_dedup_across() {
        let lib = SkillLibrary::new(vec![
            skill("a", "A", &[]),
            skill("b", "B", &["a"]),
            skill("c", "C", &["a"]),
        ]);
        let comp = lib.compose(&["b".into(), "c".into()]).expect("compose");
        assert_eq!(comp.skill_ids, vec!["a", "b", "c"]);
    }
}
