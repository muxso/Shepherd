//! 交付反馈编排:交付完成后由 Judge 按验收准则评审交付物(内置 AcceptAllJudge/RuleJudge),
//! 不通过走 Reviser 修订循环,通过后经 VerificationGateway 同步验收状态,
//! 串起 requirement → dispatch → verify 流水线的收尾一环。
//! 仅依赖 TaskGateway/VerificationGateway 等端口,自身无 IO。

pub mod application;
pub mod judges;
pub mod ports;
