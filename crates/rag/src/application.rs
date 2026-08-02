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

/// Answer `question` for `project_id` with hybrid retrieval (semantic + keyword, RRF-fused, optional
/// LLM rerank) and multi-turn `history`, tracing each stage. `rerank` gates the (slower) LLM rerank.
#[allow(clippy::too_many_arguments)]
pub async fn ask(
    store: Arc<dyn VectorStore>,
    embedder: Arc<dyn Embedder>,
    chat: Arc<dyn Chat>,
    project_id: &str,
    question: &str,
    top_k: usize,
    history: &[(String, String)],
    rerank: bool,
) -> Result<AskOutcome> {
    let overall = Instant::now();
    let top_k = top_k.clamp(1, 20);
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

    // 2) keyword search
    let t = Instant::now();
    let kw = store.keyword_search(project_id, question, top_k * 2).await.unwrap_or_default();
    trace.steps.push(TraceStep::KeywordSearch {
        query: question.to_string(),
        fetched: kw.len(),
        top: kw
            .iter()
            .take(6)
            .map(|h| TraceHit { topic: heading_or_title(h), score: h.score })
            .collect(),
        latency_ms: t.elapsed().as_millis() as u64,
    });

    // 3) semantic search
    let t = Instant::now();
    let sem = store.search(project_id, &qvec, top_k * 2).await?;
    trace.steps.push(TraceStep::SemanticSearch {
        fetched: sem.len(),
        top: sem
            .iter()
            .take(6)
            .map(|h| TraceHit { topic: heading_or_title(h), score: h.score })
            .collect(),
        latency_ms: t.elapsed().as_millis() as u64,
    });

    // 4) fuse (RRF) semantic + keyword
    let candidates = kw.len() + sem.len();
    let mut hits = rrf(&[sem, kw], 60.0, top_k.max(10));
    trace.steps.push(TraceStep::Fusion { method: "rrf", candidates, selected: hits.len() });

    // 5) optional LLM rerank of the fused candidates
    let t = Instant::now();
    let mut applied = false;
    if rerank && hits.len() > 1 {
        if let Some(order) = llm_rerank(chat.as_ref(), question, &hits).await {
            hits = reorder(hits, &order);
            applied = true;
        }
    }
    if rerank {
        trace.steps.push(TraceStep::Rerank {
            candidates: hits.len(),
            latency_ms: t.elapsed().as_millis() as u64,
            applied,
        });
    }
    hits.truncate(top_k);

    // 6) build context
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

    // 7) synthesize
    let (system, user) = build_prompt(question, &context, history);
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

/// Reciprocal-rank fusion of ranked hit lists → top-k by summed 1/(k+rank), deduped by chunk id.
fn rrf(lists: &[Vec<Hit>], k: f32, top_k: usize) -> Vec<Hit> {
    use std::collections::HashMap;
    let mut score: HashMap<String, f32> = HashMap::new();
    let mut by_id: HashMap<String, Hit> = HashMap::new();
    for list in lists {
        for (rank, h) in list.iter().enumerate() {
            *score.entry(h.chunk_id.clone()).or_insert(0.0) += 1.0 / (k + rank as f32 + 1.0);
            by_id.entry(h.chunk_id.clone()).or_insert_with(|| h.clone());
        }
    }
    let mut items: Vec<(String, f32)> = score.into_iter().collect();
    items.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    items
        .into_iter()
        .take(top_k)
        .filter_map(|(id, s)| {
            by_id.get(&id).map(|h| {
                let mut h = h.clone();
                h.score = s;
                h
            })
        })
        .collect()
}

/// Ask the LLM to rank candidates by relevance → 0-based order. None on any failure (keep fusion order).
async fn llm_rerank(chat: &dyn Chat, question: &str, cands: &[Hit]) -> Option<Vec<usize>> {
    let list = cands
        .iter()
        .enumerate()
        .map(|(i, h)| format!("[{}] {}", i + 1, h.content.chars().take(200).collect::<String>()))
        .collect::<Vec<_>>()
        .join("\n");
    let sys = "你是检索重排器。根据与问题的相关性,把候选段落编号从最相关到最不相关排序,只输出逗号分隔的编号(例:3,1,2),不要任何解释。";
    let user = format!("问题: {question}\n候选:\n{list}");
    let out = chat.complete(sys, &user).await.ok()?;
    let order: Vec<usize> = out
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse::<usize>().ok())
        .filter(|n| *n >= 1 && *n <= cands.len())
        .map(|n| n - 1)
        .collect();
    if order.is_empty() {
        None
    } else {
        Some(order)
    }
}

/// Reorder `hits` by `order` (0-based, possibly partial); any hits not listed keep their relative tail order.
fn reorder(hits: Vec<Hit>, order: &[usize]) -> Vec<Hit> {
    let mut out: Vec<Hit> = Vec::with_capacity(hits.len());
    let mut used = vec![false; hits.len()];
    for &i in order {
        if i < hits.len() && !used[i] {
            used[i] = true;
            out.push(hits[i].clone());
        }
    }
    for (i, h) in hits.into_iter().enumerate() {
        if !used[i] {
            out.push(h);
        }
    }
    out
}

/// Builds the (system, user) RAG prompt. Answer-first, cite `[n]`, never fabricate. Includes history.
pub fn build_prompt(
    question: &str,
    context: &[ContextChunk],
    history: &[(String, String)],
) -> (String, String) {
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
    let mut hist = String::new();
    if !history.is_empty() {
        hist.push_str("【对话历史】\n");
        for (role, content) in history.iter().rev().take(6).rev() {
            hist.push_str(&format!("{role}: {content}\n"));
        }
        hist.push('\n');
    }
    let user = format!("{ctx}{hist}【问题】\n{question}");
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
        let (_s, user) = build_prompt("q?", &ctx, &[]);
        assert!(user.contains("[1] (H)"));
        assert!(user.contains("【问题】\nq?"));
    }
}
