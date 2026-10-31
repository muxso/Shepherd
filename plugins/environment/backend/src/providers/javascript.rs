//! A fresh, bounded JavaScript runtime per operation. No host functions/loaders/jobs.
use async_trait::async_trait;
use plugin_runtime::*;
use rquickjs::{Context, Function, Runtime, Value};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
pub struct JavascriptProvider;
pub(crate) fn validate_variables(vars: &BTreeMap<String, String>) -> Result<(), PluginError> {
    if vars.len() > 128
        || vars
            .iter()
            .any(|(name, value)| name.is_empty() || name.len() > 128 || value.len() > 65_536)
        || vars.iter().map(|(name, value)| name.len() + value.len()).sum::<usize>() > 65_536
    {
        return Err(PluginError::InvalidConfig(
            "preparation variables exceed 128 names or 64 KiB".into(),
        ));
    }
    Ok(())
}
fn source(config: &PluginConfig) -> Result<&str, PluginError> {
    config
        .values
        .get("code")
        .and_then(serde_json::Value::as_str)
        .filter(|code| !code.trim().is_empty() && code.len() <= 65_536)
        .ok_or_else(|| {
            PluginError::InvalidConfig("provide JavaScript source, at most 64 KiB".into())
        })
}
fn run(
    code: &str,
    variables: Option<&BTreeMap<String, String>>,
    context: Option<&ExecutionContext>,
) -> Result<BTreeMap<String, String>, PluginError> {
    let deadline = Instant::now()
        + Duration::from_millis(context.map_or(100, |c| c.limits.timeout_ms.min(100)));
    let interrupted = Arc::new(AtomicBool::new(false));
    let flag = interrupted.clone();
    let cancellation = context.map(|c| c.cancellation.clone());
    let runtime = Runtime::new()
        .map_err(|_| PluginError::Execution("cannot create JavaScript runtime".into()))?;
    let asynchronous = Arc::new(AtomicBool::new(false));
    let promise_seen = asynchronous.clone();
    runtime.set_promise_hook(Some(Box::new(move |_, _, _, _| {
        promise_seen.store(true, Ordering::SeqCst)
    })));
    runtime.set_memory_limit(8 * 1024 * 1024);
    runtime.set_max_stack_size(128 * 1024);
    runtime.set_interrupt_handler(Some(Box::new(move || {
        let stop =
            Instant::now() >= deadline || cancellation.as_ref().is_some_and(|c| c.is_cancelled());
        if stop {
            flag.store(true, Ordering::SeqCst);
        }
        stop
    })));
    let ctx = Context::full(&runtime)
        .map_err(|_| PluginError::Execution("cannot create JavaScript context".into()))?;
    let result = ctx.with(|ctx| -> Result<BTreeMap<String, String>, PluginError> {
        // Calling the built-in constructor compiles the body without evaluating it.
        let constructor: Function = ctx.globals().get("Function").map_err(js_error)?;
        let function: Function =
            constructor.call(("vars", format!("\"use strict\";\n{code}"))).map_err(js_error)?;
        let Some(variables) = variables else {
            return Ok(BTreeMap::new());
        };
        validate_variables(variables)?;
        let encoded = serde_json::to_string(variables)
            .map_err(|_| PluginError::InvalidConfig("invalid variables".into()))?;
        let vars = ctx.json_parse(encoded).map_err(js_error)?;
        let returned: Value = function.call((vars.clone(),)).map_err(js_error)?;
        if returned.is_promise() {
            return Err(PluginError::UnsupportedOperation("asynchronous JavaScript".into()));
        }
        if !returned.is_undefined() {
            return Err(PluginError::Execution(
                "JavaScript must update vars and return no value".into(),
            ));
        }
        let json = ctx
            .json_stringify(vars)
            .map_err(js_error)?
            .ok_or_else(|| {
                PluginError::Execution("JavaScript variables must be a JSON object".into())
            })?
            .to_string()
            .map_err(js_error)?;
        if json.len() > 262_144 {
            return Err(PluginError::OutputLimit);
        }
        let vars = serde_json::from_str::<BTreeMap<String, String>>(&json).map_err(|_| {
            PluginError::Execution("JavaScript vars must contain string values".into())
        })?;
        validate_variables(&vars)?;
        Ok(vars)
    });
    if context.is_some_and(|c| c.cancellation.is_cancelled()) {
        return Err(PluginError::Cancelled);
    }
    if interrupted.load(Ordering::SeqCst) || Instant::now() >= deadline {
        return Err(PluginError::Timeout);
    }
    if asynchronous.load(Ordering::SeqCst) || runtime.is_job_pending() {
        return Err(PluginError::UnsupportedOperation("JavaScript jobs/promises".into()));
    }
    result
}
fn js_error(_: rquickjs::Error) -> PluginError {
    // Exceptions can contain variable/credential values; never format them.
    PluginError::Execution("JavaScript failed (syntax, runtime, stack or memory limit)".into())
}
#[async_trait]
impl EnvironmentPlugin for JavascriptProvider {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "javascript".into(),
            name: "JavaScript".into(),
            section: "pre".into(),
            version: "1.0.0".into(),
            api_version: PLUGIN_API_VERSION,
            capabilities: vec![Capability::Preprocess, Capability::Execute],
            operations: vec!["execute".into(), "plan".into()],
            config_schema: ConfigSchema {
                version: 1,
                fields: vec![ConfigField {
                    name: "code".into(),
                    label: "JavaScript (vars.name = value)".into(),
                    required: true,
                    kind: FieldType::Multiline { max_length: 65_536 },
                }],
            },
        }
    }
    fn validate_config(&self, config: &PluginConfig) -> Result<(), PluginError> {
        run(source(config)?, None, None).map(|_| ()).map_err(|_| {
            PluginError::InvalidConfig(
                "invalid JavaScript source or compilation limit exceeded".into(),
            )
        })
    }
    async fn execute(
        &self,
        invocation: &Invocation,
        context: &ExecutionContext,
    ) -> Result<PluginOutput, PluginError> {
        if invocation.operation == "plan" {
            return Ok(PluginOutput::success(
                serde_json::json!({"steps":[crate::preparation::Step::Javascript{code:source(&invocation.config)?.into()}]}),
            ));
        }
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            variables: BTreeMap<String, String>,
        }
        let input: Input = serde_json::from_value(invocation.input.clone()).map_err(|_| {
            PluginError::InvalidConfig("JavaScript input requires variables".into())
        })?;
        // Keep CPU work scoped to this poll; Tokio may replace a multi-thread
        // worker. No detached thread/job can outlive the operation.
        let code = source(&invocation.config)?;
        let evaluate = || run(code, Some(&input.variables), Some(context));
        let variables = if tokio::runtime::Handle::try_current().is_ok_and(|handle| {
            handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread
        }) {
            tokio::task::block_in_place(evaluate)
        } else {
            evaluate()
        }?;
        let mut output = PluginOutput::success(
            serde_json::json!({"variableNames":variables.keys().collect::<Vec<_>>()}),
        );
        output.variables = variables;
        Ok(output)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn javascript_has_no_io_retains_strings_and_enforces_limits() {
        let vars = [("seed".into(), "2".into())].into();
        let result =
            run("vars.answer = String(Number(vars.seed) + 3);", Some(&vars), None).expect("script");
        assert_eq!(result["answer"], "5");
        assert!(run("vars.answer = 3;", Some(&vars), None).is_err());
        assert!(run(
            "vars.answer = typeof process + ':' + typeof require + ':' + typeof fetch;",
            Some(&vars),
            None
        )
        .expect("globals")["answer"]
            .contains("undefined:undefined:undefined"));
        assert!(run("while (true) {}", Some(&vars), None).is_err());
        assert!(run("vars.answer='x'.repeat(100000000);", Some(&vars), None).is_err());
        assert!(run("function recurse(){recurse()} recurse();", Some(&vars), None).is_err());
        assert!(run("return Promise.resolve(1)", Some(&vars), None).is_err());
        assert!(matches!(
            run(
                "async function later(){throw new Error('unobserved');} later();",
                Some(&vars),
                None
            ),
            Err(PluginError::UnsupportedOperation(_))
        ));
        assert!(run("Promise.reject('unobserved');", Some(&vars), None).is_err());
        assert!(run("Promise.resolve().then(()=>vars.answer='later');", Some(&vars), None).is_err());
        assert!(run("vars.fresh = typeof leaked; globalThis.leaked = 'leak';", Some(&vars), None)
            .expect("first")["fresh"]
            .contains("undefined"));
        assert!(run("vars.fresh = typeof leaked;", Some(&vars), None).expect("isolated")["fresh"]
            .contains("undefined"));
    }
}
