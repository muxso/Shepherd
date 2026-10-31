//! Plugin-owned setup orchestration; no core tables or detached work.
use crate::{
    domain::{Binding, BindingSlot, Profile, StoreError},
    ports::ProfileStore,
    providers::javascript::{validate_variables, JavascriptProvider},
};
use api_runner::{
    domain::preparation::render, PreparationStepReport, PreparedVariables, Processor,
};
use plugin_runtime::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    Variables {
        values: BTreeMap<String, String>,
    },
    Javascript {
        code: String,
    },
    Invoke {
        #[serde(rename = "profileId")]
        profile_id: String,
        input: Value,
        #[serde(rename = "resultVariable", default)]
        result_variable: Option<String>,
    },
    Wait {
        ms: u64,
    },
}
impl Step {
    pub(crate) fn validate(&self) -> Result<(), PluginError> {
        match self {
            Self::Variables { values } => validate_variables(values),
            Self::Javascript { code } => JavascriptProvider.validate_config(&script_config(code)),
            Self::Invoke { profile_id, input, result_variable } => {
                if profile_id.trim().is_empty()
                    || profile_id.len() > 128
                    || serde_json::to_vec(input)
                        .map_err(|_| PluginError::InvalidConfig("invalid setup input".into()))?
                        .len()
                        > 1_048_576
                    || result_variable
                        .as_ref()
                        .is_some_and(|name| name.is_empty() || name.len() > 128)
                {
                    Err(PluginError::InvalidConfig(
                        "invalid setup invocation reference/input".into(),
                    ))
                } else {
                    Ok(())
                }
            }
            Self::Wait { ms } => {
                if *ms <= 30_000 {
                    Ok(())
                } else {
                    Err(PluginError::InvalidConfig("setup waits may not exceed 30000 ms".into()))
                }
            }
        }
    }
    fn kind(&self) -> &'static str {
        match self {
            Self::Variables { .. } => "variables",
            Self::Javascript { .. } => "javascript",
            Self::Invoke { .. } => "invoke",
            Self::Wait { .. } => "wait",
        }
    }
}
fn script_config(code: &str) -> PluginConfig {
    PluginConfig {
        schema_version: 1,
        values: [("code".into(), json!(code))].into(),
        secrets: BTreeMap::new(),
    }
}
fn store_error(error: StoreError) -> PluginError {
    match error {
        StoreError::NotFound => {
            PluginError::InvalidConfig("setup references an unavailable profile".into())
        }
        _ => PluginError::Unavailable("setup profile store".into()),
    }
}
enum Operation {
    Variables { values: BTreeMap<String, String> },
    Javascript { code: String },
    Invoke { profile: Box<Profile>, input: Value, result_variable: Option<String> },
    Wait { ms: u64 },
}
struct Planned {
    source: String,
    kind: &'static str,
    operation: Operation,
}
pub struct Preparation<'a> {
    pub store: &'a dyn ProfileStore,
    pub registry: &'a PluginRegistry,
    pub context: &'a ExecutionContext,
}
impl Preparation<'_> {
    pub async fn run(
        &self,
        variables: BTreeMap<String, String>,
        processors: &[Processor],
    ) -> Result<PreparedVariables, PluginError> {
        if self.context.cancellation.is_cancelled() {
            return Err(PluginError::Cancelled);
        }
        self.context.scope.validate()?;
        self.context.limits.validate()?;
        validate_variables(&variables)?;
        let result = tokio::select! {
            biased;
            _=self.context.cancellation.cancelled()=>return Err(PluginError::Cancelled),
            result=tokio::time::timeout(Duration::from_millis(self.context.limits.timeout_ms),self.run_inner(variables,processors))=>result.map_err(|_|PluginError::Timeout)??,
        };
        if serde_json::to_vec(&result).map_err(|_| PluginError::OutputLimit)?.len()
            > self.context.limits.max_output_bytes
        {
            return Err(PluginError::OutputLimit);
        }
        Ok(result)
    }
    async fn plan_profile(&self, id: &str) -> Result<Vec<Step>, PluginError> {
        let profile =
            self.store.get(&self.context.scope.project_id, id).await.map_err(store_error)?;
        if !profile.enabled {
            return Err(PluginError::Unavailable("setup profile disabled".into()));
        }
        self.registry.validate_config(&profile.plugin_id, &profile.config)?;
        let descriptor = self
            .registry
            .descriptors()
            .into_iter()
            .find(|d| d.id == profile.plugin_id)
            .ok_or_else(|| PluginError::Unavailable("setup provider".into()))?;
        if !descriptor.capabilities.contains(&Capability::Preprocess) {
            return Err(PluginError::InvalidConfig("profile does not support preparation".into()));
        }
        self.context
            .authorization
            .authorize(&self.context.scope, &profile.plugin_id, "execute")
            .await?;
        let output = self
            .registry
            .execute(
                &Invocation {
                    plugin_id: profile.plugin_id,
                    operation: "plan".into(),
                    config: profile.config,
                    input: Value::Null,
                },
                self.context,
            )
            .await?;
        if output.status != OutputStatus::Success {
            return Err(PluginError::Execution("setup planning failed".into()));
        }
        let steps: Vec<Step> =
            serde_json::from_value(output.data.get("steps").cloned().ok_or_else(|| {
                PluginError::InvalidConfig("setup provider returned no steps".into())
            })?)
            .map_err(|_| {
                PluginError::InvalidConfig("setup provider returned invalid steps".into())
            })?;
        if steps.is_empty() || steps.len() > 32 {
            return Err(PluginError::InvalidConfig("setup provider must return 1–32 steps".into()));
        }
        Ok(steps)
    }
    async fn data_source(
        &self,
        profile_id: &str,
        bindings: &[Binding],
    ) -> Result<Profile, PluginError> {
        if !bindings
            .iter()
            .any(|b| b.enabled && b.slot == BindingSlot::Connection && b.profile_id == profile_id)
        {
            return Err(PluginError::PermissionDenied);
        }
        let profile = self
            .store
            .get(&self.context.scope.project_id, profile_id)
            .await
            .map_err(store_error)?;
        if !profile.enabled {
            return Err(PluginError::Unavailable("setup data source disabled".into()));
        }
        let descriptor = self
            .registry
            .descriptors()
            .into_iter()
            .find(|d| d.id == profile.plugin_id)
            .ok_or_else(|| PluginError::Unavailable("setup data source provider".into()))?;
        if !descriptor.capabilities.contains(&Capability::Execute)
            || !descriptor.operations.iter().any(|op| op == "execute")
        {
            return Err(PluginError::UnsupportedOperation("setup data source execution".into()));
        }
        self.registry.validate_config(&profile.plugin_id, &profile.config)?;
        self.context
            .authorization
            .authorize(&self.context.scope, &profile.plugin_id, "execute")
            .await?;
        for reference in profile.config.secrets.values() {
            self.context.secrets.resolve(&self.context.scope, reference).await?;
        }
        Ok(profile)
    }
    async fn run_inner(
        &self,
        variables: BTreeMap<String, String>,
        processors: &[Processor],
    ) -> Result<PreparedVariables, PluginError> {
        let started = Instant::now();
        let scope = &self.context.scope;
        self.context.authorization.authorize(scope, "preparation", "execute").await?;
        let mut bindings = self
            .store
            .bindings(&scope.project_id, &scope.environment_id)
            .await
            .map_err(store_error)?;
        bindings.sort_by(|a, b| {
            a.priority.cmp(&b.priority).then_with(|| a.profile_id.cmp(&b.profile_id))
        });
        let mut pending = Vec::new();
        for binding in bindings.iter().filter(|b| b.enabled && b.slot == BindingSlot::Pre) {
            let profile = self
                .store
                .get(&scope.project_id, &binding.profile_id)
                .await
                .map_err(store_error)?;
            if !profile.enabled {
                continue;
            }
            for step in self.plan_profile(&binding.profile_id).await? {
                pending.push((format!("environment:{}", binding.profile_id), step));
            }
        }
        for (index, processor) in processors.iter().enumerate() {
            let steps = match processor {
                Processor::Plugin { profile_id } => self.plan_profile(profile_id).await?,
                Processor::Wait { ms } => vec![Step::Wait { ms: *ms }],
                Processor::Script { lang, code }
                    if lang.eq_ignore_ascii_case("JS")
                        || lang.eq_ignore_ascii_case("JAVASCRIPT") =>
                {
                    vec![Step::Javascript { code: code.clone() }]
                }
                Processor::Script { .. } => {
                    return Err(PluginError::UnsupportedOperation(
                        "setup script language; only JavaScript is supported".into(),
                    ))
                }
                Processor::Sql { name, datasource, sql } => vec![Step::Invoke {
                    profile_id: datasource.clone(),
                    input: json!({"query":sql}),
                    result_variable: (!name.is_empty()).then(|| name.clone()),
                }],
                Processor::Extract { .. } => {
                    return Err(PluginError::InvalidConfig(
                        "response extraction belongs in post-processing".into(),
                    ))
                }
            };
            for step in steps {
                pending.push((format!("case:{}", index + 1), step));
            }
        }
        if pending.len() > 128 {
            return Err(PluginError::InvalidConfig(
                "preparation may contain at most 128 steps".into(),
            ));
        }
        // Preflight all declarations and scoped credentials before any step performs IO.
        let mut planned = Vec::new();
        for (source, step) in pending {
            step.validate()?;
            let kind = step.kind();
            let operation = match step {
                Step::Variables { values } => Operation::Variables { values },
                Step::Javascript { code } => Operation::Javascript { code },
                Step::Wait { ms } => Operation::Wait { ms },
                Step::Invoke { profile_id, input, result_variable } => Operation::Invoke {
                    profile: Box::new(self.data_source(&profile_id, &bindings).await?),
                    input,
                    result_variable,
                },
            };
            planned.push(Planned { source, kind, operation });
        }
        let mut result = PreparedVariables { variables, steps: Vec::new() };
        for (index, planned) in planned.into_iter().enumerate() {
            if self.context.cancellation.is_cancelled() {
                return Err(PluginError::Cancelled);
            }
            let remaining =
                self.context.limits.timeout_ms.saturating_sub(started.elapsed().as_millis() as u64);
            if remaining == 0 {
                return Err(PluginError::Timeout);
            }
            let mut context = self.context.clone();
            context.limits.timeout_ms = remaining;
            let step_started = Instant::now();
            let kind = planned.kind;
            let outcome: Result<(), PluginError> = async {
                match planned.operation {
                    Operation::Variables { values } => {
                        let rendered = values
                            .into_iter()
                            .map(|(name, value)| {
                                Ok((
                                    name,
                                    render(&value, &result.variables, 65_536)
                                        .map_err(|e| PluginError::InvalidConfig(e.to_string()))?,
                                ))
                            })
                            .collect::<Result<BTreeMap<_, _>, PluginError>>()?;
                        result.variables.extend(rendered);
                    }
                    Operation::Javascript { code } => {
                        let output = self
                            .registry
                            .execute(
                                &Invocation {
                                    plugin_id: "javascript".into(),
                                    operation: "execute".into(),
                                    config: script_config(&code),
                                    input: json!({"variables":result.variables}),
                                },
                                &context,
                            )
                            .await?;
                        if output.status != OutputStatus::Success {
                            return Err(PluginError::Execution(
                                "JavaScript preparation failed".into(),
                            ));
                        }
                        result.variables = output.variables;
                    }
                    Operation::Invoke { profile, input, result_variable } => {
                        let input =
                            render_json(input, &result.variables, context.limits.max_output_bytes)?;
                        let output = self
                            .registry
                            .execute(
                                &Invocation {
                                    plugin_id: profile.plugin_id,
                                    operation: "execute".into(),
                                    config: profile.config,
                                    input,
                                },
                                &context,
                            )
                            .await?;
                        if output.status != OutputStatus::Success {
                            return Err(PluginError::Execution(
                                "data-source preparation returned a failure".into(),
                            ));
                        }
                        result.variables.extend(output.variables);
                        if let Some(name) = result_variable {
                            result.variables.insert(
                                name,
                                serde_json::to_string(&output.data)
                                    .map_err(|_| PluginError::OutputLimit)?,
                            );
                        }
                    }
                    Operation::Wait { ms } => tokio::time::sleep(Duration::from_millis(ms)).await,
                }
                validate_variables(&result.variables)
            }
            .await;
            if let Err(error) = outcome {
                return Err(match error {
                    PluginError::Timeout
                    | PluginError::Cancelled
                    | PluginError::PermissionDenied => error,
                    _ => PluginError::Execution(format!(
                        "preparation step {} ({kind}, {}) failed: {error}",
                        index + 1,
                        planned.source
                    )),
                });
            }
            result.steps.push(PreparationStepReport {
                source: planned.source,
                kind: kind.into(),
                elapsed_ms: step_started.elapsed().as_millis() as u64,
                variable_names: result.variables.keys().cloned().collect(),
            });
        }
        Ok(result)
    }
}
fn render_json(
    value: Value,
    vars: &BTreeMap<String, String>,
    max: usize,
) -> Result<Value, PluginError> {
    fn charge(remaining: &mut usize, bytes: usize) -> Result<(), PluginError> {
        *remaining = remaining.checked_sub(bytes).ok_or(PluginError::OutputLimit)?;
        Ok(())
    }
    fn visit(
        value: Value,
        vars: &BTreeMap<String, String>,
        remaining: &mut usize,
        nodes: &mut usize,
    ) -> Result<Value, PluginError> {
        charge(nodes, 1)?;
        charge(remaining, 2)?;
        Ok(match value {
            Value::String(text) => {
                let text = render(&text, vars, *remaining)
                    .map_err(|e| PluginError::InvalidConfig(e.to_string()))?;
                charge(remaining, text.len())?;
                Value::String(text)
            }
            Value::Array(values) => Value::Array(
                values
                    .into_iter()
                    .map(|v| visit(v, vars, remaining, nodes))
                    .collect::<Result<_, _>>()?,
            ),
            Value::Object(values) => Value::Object(
                values
                    .into_iter()
                    .map(|(k, v)| {
                        charge(remaining, k.len())?;
                        Ok((k, visit(v, vars, remaining, nodes)?))
                    })
                    .collect::<Result<_, PluginError>>()?,
            ),
            other => {
                charge(remaining, other.to_string().len())?;
                other
            }
        })
    }
    let value = visit(value, vars, &mut { max }, &mut 10_000)?;
    if serde_json::to_vec(&value).map_err(|_| PluginError::OutputLimit)?.len() > max {
        return Err(PluginError::OutputLimit);
    }
    Ok(value)
}
