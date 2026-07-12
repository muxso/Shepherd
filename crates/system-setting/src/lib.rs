//! System-setting context: users/organizations/roles and authentication. Roles are scoped at
//! three levels (System/Organization/Project). Login supports three paths: local password
//! (auth), LDAP directory (ldap), and OIDC external identity (oidc), plus API key management.
//! domain/application/ports do no IO; pg/http and the auth adapters are feature-gated.

pub mod adapters;
pub mod application;
pub mod domain;
pub mod ports;
