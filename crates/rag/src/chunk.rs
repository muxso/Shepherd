//! Markdown chunker. Splits on heading boundaries, keeps a heading breadcrumb per chunk, caps
//! chunk size, and carries a small overlap between consecutive chunks so context isn't cut mid-idea.
//! Adapted from the source RAG's `UnifiedDocument::to_chunks` (breadcrumb + ~10-20% overlap).

/// One chunk: a heading breadcrumb ("Doc › H1 › H2") and its body text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub heading: String,
    pub content: String,
}

const OVERLAP_CHARS: usize = 150;

/// Chunk `text` (markdown) under `title`. `max_chars` bounds each chunk's body (~2 chars/token).
pub fn chunk_markdown(title: &str, text: &str, max_chars: usize) -> Vec<Chunk> {
    let max_chars = max_chars.max(200);
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut stack: Vec<(u8, String)> = Vec::new(); // (level, heading)
    let mut section = String::new();

    let breadcrumb = |stack: &[(u8, String)]| -> String {
        let mut parts: Vec<&str> = vec![title];
        parts.extend(stack.iter().map(|(_, h)| h.as_str()));
        parts.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" › ")
    };

    for line in text.lines() {
        if let Some((level, heading)) = parse_heading(line) {
            if !section.trim().is_empty() {
                push_section(&mut chunks, &breadcrumb(&stack), &section, max_chars);
                section.clear();
            }
            stack.retain(|(l, _)| *l < level);
            stack.push((level, heading));
            continue;
        }
        section.push_str(line);
        section.push('\n');
        if section.chars().count() >= max_chars {
            push_section(&mut chunks, &breadcrumb(&stack), &section, max_chars);
            // carry a short overlap tail into the next section
            let tail: String = section
                .chars()
                .rev()
                .take(OVERLAP_CHARS)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            section = tail;
        }
    }
    if !section.trim().is_empty() {
        push_section(&mut chunks, &breadcrumb(&stack), &section, max_chars);
    }
    chunks
}

fn parse_heading(line: &str) -> Option<(u8, String)> {
    let t = line.trim_start();
    if !t.starts_with('#') {
        return None;
    }
    let level = t.chars().take_while(|c| *c == '#').count().min(6) as u8;
    let heading = t.trim_start_matches('#').trim().to_string();
    if heading.is_empty() {
        None
    } else {
        Some((level, heading))
    }
}

fn push_section(chunks: &mut Vec<Chunk>, crumb: &str, body: &str, max_chars: usize) {
    let body = body.trim();
    if body.is_empty() {
        return;
    }
    for piece in split_capped(body, max_chars) {
        chunks.push(Chunk { heading: crumb.to_string(), content: piece });
    }
}

/// Split an oversized body into <= max_chars pieces on soft boundaries (paragraph → newline → char).
fn split_capped(body: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 || body.chars().count() <= max_chars {
        return vec![body.to_string()];
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    for para in body.split("\n\n") {
        if cur.chars().count() + para.chars().count() > max_chars && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        if para.chars().count() > max_chars {
            // hard-split a single huge paragraph
            let mut buf = String::new();
            for ch in para.chars() {
                buf.push(ch);
                if buf.chars().count() >= max_chars {
                    out.push(std::mem::take(&mut buf));
                }
            }
            if !buf.is_empty() {
                cur.push_str(&buf);
            }
        } else {
            if !cur.is_empty() {
                cur.push_str("\n\n");
            }
            cur.push_str(para);
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_headings_with_breadcrumb() {
        let md = "# Intro\nhello world\n\n## Details\nsome detail text here";
        let chunks = chunk_markdown("Doc", md, 500);
        assert!(chunks.len() >= 2);
        assert!(chunks[0].heading.contains("Doc"));
        assert!(chunks.iter().any(|c| c.heading.contains("Details")));
    }

    #[test]
    fn caps_oversized_section() {
        let big = "x".repeat(2000);
        let chunks = chunk_markdown("Doc", &big, 500);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.content.chars().count() <= 500 + 10));
    }
}
