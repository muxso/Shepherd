//! API definition context: the ApiDefinition aggregate and module tree (ApiModule), with attached API cases (ApiCase) and mock definitions (ApiMock).
//! Supports import from five formats (OpenAPI/Postman/HAR/JMeter/MeterSphere); ImportSchedule defines scheduled sync imports.
//! domain/application/ports do no IO; pg/http adapters are enabled by same-named features.

// Import-parser module docs contain nested lists; allow the continuation-indent lint.
#![allow(clippy::doc_lazy_continuation)]

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
