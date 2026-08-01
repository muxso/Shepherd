//! RAG: knowledge-base retrieval-augmented Q&A for a project.
//!
//! Pipeline: ingest (chunk → embed → store) and ask (embed → cosine retrieve → context → synthesize),
//! with a decision-chain trace (AskTrace/TraceStep) surfaced to the Q&A UI. Storage is plain Postgres
//! (embeddings as real[] ranked by the in-DB `rag_cosine` function); the VectorStore port keeps a
//! pgvector-backed store a drop-in swap.

pub mod application;
pub mod chunk;
pub mod domain;
pub mod ports;

#[cfg(feature = "http")]
pub mod adapters;
#[cfg(feature = "http")]
pub mod http;
