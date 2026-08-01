//! RAG use cases: ingest (chunk → embed → store) and ask (embed → retrieve → context → synthesize),
//! the latter capturing a decision-chain trace at each stage.

use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::chunk::chunk_markdown;
use crate::domain::{
    Answer, AskTrace, ContextChunk, Hit, RagChunk, RagDocument, Result, TraceContextChunk,
    TraceHit, TraceStep,
};
use crate::ports::{Chat, Embedder, VectorStore};

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Default per-chunk body cap (~512 tokens at ~2 chars/token).
pub const CHUNK_MAX_CHARS: usize = 1024;

/// Ingest a document: (re)chunk its markdown, embed each chunk, and replace its stored chunks.
#[allow(clippy::too_many_arguments)]
pub async fn ingest(
    store: &dyn VectorStore,
    embedder: &dyn Embedder,
    doc: RagDocument,
    text: &str,
) -> Result<usize> {
    let pieces = chunk_markdown(&doc.title, text, CHUNK_MAX_CHARS);
    // Prepend the heading breadcrumb to the embedded text so retrieval has that context.
    let texts: Vec<String> =
        pieces.iter().map(|c| format!("{}\n{}", c.heading, c.content)).collect();
    let embeddings =
        if texts.is_empty() { Vec::new() } else { embedder.embed_batch(&texts).await? };

    let ts = now_ms();
    let chunks: Vec<RagChunk> = pieces
        .into_iter()
        .zip(embeddings)
        .enumerate()
        .map(|(i, (c, emb))| RagChunk {
            id: uuid::Uuid::new_v4().to_string(),
            document_id: doc.id.clone(),
            project_id: doc.project_id.clone(),
            chunk_index: i as i32,
            heading: c.heading,
            content: c.content,
            embedding: emb,
            created_at: ts,
        })
        .collect();

    store.upsert_document(&doc).await?;
    let n = chunks.len();
    store.replace_chunks(&doc.id, &chunks).await?;
    Ok(n)
}

pub struct AskOutcome {
    pub answer: Answer,
    pub hits: Vec<Hit>,
    pub trace: AskTrace,
}

/// Answer `question` for `project_id`: embed → cosine top-k → build context → synthesize, tracing each step.
pub async fn ask(
    store: Arc<dyn VectorStore>,
    embedder: Arc<dyn Embedder>,
    chat: Arc<dyn Chat>,
    project_id: &str,
    question: &str,
    top_k: usize,
) -> Result<AskOutcome> {
    let overall = Instant::now();
    let mut trace = AskTrace {
        question: question.to_string(),
        started_at: now_ms().to_string(),
        steps: Vec::new(),
        total_ms: 0,
        channel: "ask",
    };

    // 1) embed the query
    let t = Instant::now();
    let qvec = embedder.embed(question).await?;
    trace
        .steps
        .push(TraceStep::Embedding { dim: qvec.len(), latency_ms: t.elapsed().as_millis() as u64 });

    // 2) semantic search
    let t = Instant::now();
    let hits = store.search(project_id, &qvec, top_k.clamp(1, 20)).await?;
    trace.steps.push(TraceStep::SemanticSearch {
        fetched: hits.len(),
        top: hits
            .iter()
            .take(8)
            .map(|h| TraceHit { topic: heading_or_title(h), score: h.score })
            .collect(),
        latency_ms: t.elapsed().as_millis() as u64,
    });

    // 3) build context
    let context: Vec<ContextChunk> = hits
        .iter()
        .enumerate()
        .map(|(i, h)| ContextChunk {
            index: i,
            content: h.content.clone(),
            source_title: h.title.clone(),
            heading: h.heading.clone(),
            document_id: h.document_id.clone(),
        })
        .collect();
    let approx_tokens = context.iter().map(|c| c.content.chars().count() / 2).sum();
    trace.steps.push(TraceStep::ContextBuilt {
        chunks: context
            .iter()
            .map(|c| TraceContextChunk { index: c.index, topic: topic_of(c) })
            .collect(),
        approx_tokens,
    });

    // 4) synthesize
    let (system, user) = build_prompt(question, &context);
    let t = Instant::now();
    let answer_text = chat.complete(&system, &user).await?;
    trace.steps.push(TraceStep::LlmGeneration {
        latency_ms: t.elapsed().as_millis() as u64,
        answer_chars: answer_text.chars().count(),
    });

    trace.total_ms = overall.elapsed().as_millis() as u64;
    let cited = extract_citations(&answer_text);
    Ok(AskOutcome { answer: Answer { answer: answer_text, cited_sources: cited }, hits, trace })
}

fn heading_or_title(h: &Hit) -> String {
    if !h.heading.is_empty() {
        h.heading.clone()
    } else {
        h.title.clone()
    }
}

fn topic_of(c: &ContextChunk) -> String {
    if !c.heading.is_empty() {
        c.heading.clone()
    } else {
        c.source_title.clone()
    }
}

/// Builds the (system, user) RAG prompt. Answer-first, cite `[n]`, never fabricate.
pub fn build_prompt(question: &str, context: &[ContextChunk]) -> (String, String) {
    let system = "你是知识库问答助手。规则:\n\
        1. 仅使用【上下文】回答,并用编号 [1][2] 标注来源。严禁编造上下文之外的信息。\n\
        2. 上下文中只要有与问题相关的内容,就直接给出答案(即使是部分答案)。\n\
        3. 仅当上下文完全为空或彻底不相关时,才回复\"未在知识库中找到相关内容\",并反问一个具体问题。\n\
        4. 展开关键细节:步骤、输入/输出、决策点、异常与注意事项。\n\
        5. 语言与问题一致(中文问→中文答)。"
        .to_string();

    let mut ctx = String::from("【上下文】\n");
    if context.is_empty() {
        ctx.push_str("(无)\n");
    } else {
        for c in context {
            let head =
                if c.heading.is_empty() { c.source_title.clone() } else { c.heading.clone() };
            ctx.push_str(&format!("[{}] ({})\n{}\n\n", c.index + 1, head, c.content));
        }
    }
    let user = format!("{ctx}【问题】\n{question}");
    (system, user)
}

/// Pull 1-based `[n]` citation indices from the answer, de-duplicated, in order.
pub fn extract_citations(answer: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let bytes = answer.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 && j < bytes.len() && bytes[j] == b']' {
                if let Ok(n) = answer[i + 1..j].parse::<usize>() {
                    if n > 0 && !out.contains(&n) {
                        out.push(n);
                    }
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn citations_parsed_unique_in_order() {
        assert_eq!(extract_citations("see [2] and [1], again [2]"), vec![2, 1]);
        assert_eq!(extract_citations("no refs"), Vec::<usize>::new());
    }

    #[test]
    fn prompt_numbers_sources_from_one() {
        let ctx = vec![ContextChunk {
            index: 0,
            content: "hello".into(),
            source_title: "Doc".into(),
            heading: "H".into(),
            document_id: "d".into(),
        }];
        let (_s, user) = build_prompt("q?", &ctx);
        assert!(user.contains("[1] (H)"));
        assert!(user.contains("【问题】\nq?"));
    }
}
