//! api-scenario —— 接口场景编排与"打通执行"。
//!
//! 场景(scenario)是一组有序步骤的编排:每步引用内联请求(REQUEST)、接口用例(CASE)
//! 或子场景(SCENARIO,可嵌套),并带引用模式(REFERENCE 活链接 / COPY 内联快照)。
//! 明星是 **compile(打通执行)**:递归展开子场景,把整棵编排压平成一串可运行步骤
//! —— 每步要么交出一个 case_id 给既有 api-test 批量 runner,要么是一个内联请求。
//! 递归带环检测与最大深度上限。本 crate 不依赖任何业务 crate:CASE 仅携带 case_id 字符串,
//! COPY 快照为客户端提供的 JSON。
//!
//! 六边形架构(ports & adapters):domain/ports/application 零 IO,IO 只落在 adapters。

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
