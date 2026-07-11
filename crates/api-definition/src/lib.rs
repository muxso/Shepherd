//! 接口定义上下文:ApiDefinition 聚合与模块树(ApiModule),挂接接口用例(ApiCase)与 Mock 定义(ApiMock)。
//! 支持 OpenAPI/Postman/HAR/JMeter/MeterSphere 五种格式导入,ImportSchedule 定义定时同步导入。
//! domain/application/ports 不做 IO;pg/http 适配器由同名 feature 启用。

// 导入解析模块的文档含多级列表,放行延续行缩进 lint。
#![allow(clippy::doc_lazy_continuation)]

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
