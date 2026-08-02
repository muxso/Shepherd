//! Tantivy-backed keyword index for hybrid retrieval, with a jieba tokenizer for Chinese
//! segmentation. Held in RAM and (re)built from the stored chunks; queried per project.

use std::sync::{Arc, Mutex};

use jieba_rs::Jieba;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, STORED, STRING,
};
use tantivy::tokenizer::{Token, TokenStream, Tokenizer};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

use crate::domain::{RagError, Result};

// ---- jieba tokenizer (ported from the source kb-search) ----

#[derive(Clone)]
struct JiebaTokenizer {
    jieba: Arc<Jieba>,
}
impl JiebaTokenizer {
    fn new() -> Self {
        Self { jieba: Arc::new(Jieba::new()) }
    }
}
struct JiebaStream {
    tokens: Vec<Token>,
    index: usize,
}
impl TokenStream for JiebaStream {
    fn advance(&mut self) -> bool {
        if self.index < self.tokens.len() {
            self.index += 1;
            true
        } else {
            false
        }
    }
    fn token(&self) -> &Token {
        &self.tokens[self.index - 1]
    }
    fn token_mut(&mut self) -> &mut Token {
        &mut self.tokens[self.index - 1]
    }
}
impl Tokenizer for JiebaTokenizer {
    type TokenStream<'a> = JiebaStream;
    fn token_stream(&mut self, text: &str) -> JiebaStream {
        let mut tokens = Vec::new();
        let mut offset = 0usize;
        for w in self.jieba.cut(text, false) {
            let w = w.trim();
            if !w.is_empty() {
                let start = text[offset..].find(w).map(|p| offset + p).unwrap_or(offset);
                tokens.push(Token {
                    offset_from: start,
                    offset_to: start + w.len(),
                    position: tokens.len(),
                    text: w.to_lowercase(),
                    position_length: 1,
                });
                offset = start + w.len();
            }
        }
        JiebaStream { tokens, index: 0 }
    }
}

const TOK: &str = "jieba";

struct Fields {
    chunk_id: Field,
    doc_id: Field,
    project_id: Field,
    heading: Field,
    content: Field,
}

/// A RAM keyword index over chunks; `add`/`delete` mutate it, `search` queries within a project.
pub struct TantivyKeyword {
    index: Index,
    reader: IndexReader,
    writer: Arc<Mutex<IndexWriter>>,
    f: Fields,
}

impl TantivyKeyword {
    pub fn new() -> Result<Self> {
        let mut sb = Schema::builder();
        let text = TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(TOK)
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        );
        let f = Fields {
            chunk_id: sb.add_text_field("chunk_id", STRING | STORED),
            doc_id: sb.add_text_field("doc_id", STRING | STORED),
            project_id: sb.add_text_field("project_id", STRING | STORED),
            heading: sb.add_text_field("heading", text.clone()),
            content: sb.add_text_field("content", text),
        };
        let index = Index::create_in_ram(sb.build());
        index.tokenizers().register(TOK, JiebaTokenizer::new());
        let writer = index.writer(50_000_000).map_err(be)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(be)?;
        Ok(Self { index, reader, writer: Arc::new(Mutex::new(writer)), f })
    }

    fn add_doc(
        &self,
        w: &IndexWriter,
        chunk_id: &str,
        doc_id: &str,
        project_id: &str,
        heading: &str,
        content: &str,
    ) -> Result<()> {
        let mut d = TantivyDocument::default();
        d.add_text(self.f.chunk_id, chunk_id);
        d.add_text(self.f.doc_id, doc_id);
        d.add_text(self.f.project_id, project_id);
        d.add_text(self.f.heading, heading);
        d.add_text(self.f.content, content);
        w.add_document(d).map_err(be)?;
        Ok(())
    }

    /// Replace a document's chunks: delete old rows for `doc_id`, add the new ones, commit.
    /// Each chunk is `(chunk_id, project_id, heading, content)`.
    pub fn replace_doc(
        &self,
        doc_id: &str,
        chunks: &[(String, String, String, String)],
    ) -> Result<()> {
        let mut w =
            self.writer.lock().map_err(|_| RagError::Backend("tantivy writer poisoned".into()))?;
        w.delete_term(Term::from_field_text(self.f.doc_id, doc_id));
        for (chunk_id, project_id, heading, content) in chunks {
            self.add_doc(&w, chunk_id, doc_id, project_id, heading, content)?;
        }
        w.commit().map_err(be)?;
        self.reader.reload().map_err(be)?;
        Ok(())
    }

    /// Bulk-add chunks (startup rebuild), `(chunk_id, doc_id, project_id, heading, content)`, one commit.
    pub fn bulk_add(&self, rows: &[(String, String, String, String, String)]) -> Result<()> {
        let mut w =
            self.writer.lock().map_err(|_| RagError::Backend("tantivy writer poisoned".into()))?;
        w.delete_all_documents().map_err(be)?;
        for (chunk_id, doc_id, project_id, heading, content) in rows {
            self.add_doc(&w, chunk_id, doc_id, project_id, heading, content)?;
        }
        w.commit().map_err(be)?;
        self.reader.reload().map_err(be)?;
        Ok(())
    }

    pub fn delete_doc(&self, doc_id: &str) -> Result<()> {
        let mut w =
            self.writer.lock().map_err(|_| RagError::Backend("tantivy writer poisoned".into()))?;
        w.delete_term(Term::from_field_text(self.f.doc_id, doc_id));
        w.commit().map_err(be)?;
        self.reader.reload().map_err(be)?;
        Ok(())
    }

    /// Keyword search within `project_id`. Returns (chunk_id, score) pairs, best first.
    pub fn search(
        &self,
        project_id: &str,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<(String, f32)>> {
        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(&self.index, vec![self.f.content, self.f.heading]);
        let parsed = match parser.parse_query(query) {
            Ok(q) => q,
            Err(_) => {
                // Fall back to a lenient parse of the raw terms.
                let escaped: String = query
                    .chars()
                    .filter(|c| c.is_alphanumeric() || c.is_whitespace() || *c > '\u{7f}')
                    .collect();
                match parser.parse_query(&escaped) {
                    Ok(q) => q,
                    Err(_) => return Ok(Vec::new()),
                }
            }
        };
        let proj = TermQuery::new(
            Term::from_field_text(self.f.project_id, project_id),
            IndexRecordOption::Basic,
        );
        let bool_q = BooleanQuery::new(vec![
            (Occur::Must, Box::new(proj) as Box<dyn Query>),
            (Occur::Must, parsed),
        ]);
        let hits = searcher.search(&bool_q, &TopDocs::with_limit(top_k)).map_err(be)?;
        let mut out = Vec::with_capacity(hits.len());
        for (score, addr) in hits {
            let doc: TantivyDocument = searcher.doc(addr).map_err(be)?;
            if let Some(cid) = doc.get_first(self.f.chunk_id).and_then(|v| v.as_str()) {
                out.push((cid.to_string(), score));
            }
        }
        Ok(out)
    }
}

fn be(e: impl std::fmt::Display) -> RagError {
    RagError::Backend(format!("tantivy: {e}"))
}
