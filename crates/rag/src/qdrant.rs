//! Optional ANN side index. PostgreSQL remains authoritative for text, project and visibility.
use crate::domain::{RagChunk, RagError, Result};
use serde_json::{json, Value};

#[derive(Clone)]
pub(crate) struct QdrantIndex {
    url: String,
    key: String,
    client: reqwest::Client,
}
impl QdrantIndex {
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("SHEPHERD_RAG_QDRANT_COLLECTION_URL").ok()?;
        if url.trim().is_empty() {
            return None;
        }
        Some(Self {
            url: url.trim_end_matches('/').into(),
            key: std::env::var("SHEPHERD_RAG_QDRANT_KEY").unwrap_or_default(),
            client: reqwest::Client::new(),
        })
    }
    async fn request(&self, method: reqwest::Method, path: &str, body: Value) -> Result<Value> {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.url))
            .timeout(std::time::Duration::from_secs(60))
            .json(&body);
        if !self.key.is_empty() {
            request = request.header("api-key", &self.key);
        }
        let response =
            request.send().await.map_err(|_| RagError::Backend("Qdrant unavailable".into()))?;
        if !response.status().is_success() {
            return Err(RagError::Backend(format!("Qdrant HTTP {}", response.status())));
        }
        response.json().await.map_err(|_| RagError::Backend("Invalid Qdrant response".into()))
    }
    pub async fn upsert(&self, chunks: &[RagChunk]) -> Result<()> {
        let points: Vec<Value> = chunks
            .iter()
            .filter(|c| !c.embedding.is_empty())
            .map(|c| {
                json!({"id": c.id, "vector": c.embedding,
                "payload": {"project_id": c.project_id, "document_id": c.document_id}})
            })
            .collect();
        if !points.is_empty() {
            self.request(reqwest::Method::PUT, "/points?wait=true", json!({"points": points}))
                .await?;
        }
        Ok(())
    }
    pub async fn delete_document(&self, document: &str) -> Result<()> {
        self.request(
            reqwest::Method::POST,
            "/points/delete?wait=true",
            json!({
                "filter": {"must": [{"key": "document_id", "match": {"value": document}}]}
            }),
        )
        .await?;
        Ok(())
    }
    pub async fn search(
        &self,
        project: &str,
        vector: &[f32],
        limit: usize,
    ) -> Result<Vec<(String, f32)>> {
        let response = self
            .request(
                reqwest::Method::POST,
                "/points/search",
                json!({
                    "vector": vector, "limit": limit, "with_payload": false, "with_vector": false,
                    "filter": {"must": [{"key": "project_id", "match": {"value": project}}]}
                }),
            )
            .await?;
        let rows = response["result"]
            .as_array()
            .ok_or_else(|| RagError::Backend("Qdrant missing results".into()))?;
        Ok(rows
            .iter()
            .filter_map(|r| Some((r["id"].as_str()?.into(), r["score"].as_f64()? as f32)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn side_index_lifecycle_filters_project_and_stores_no_source_text() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        let worker = std::thread::spawn(move || {
            for step in 0..3 {
                let (mut socket, _) = listener.accept().expect("accept");
                socket.set_read_timeout(Some(std::time::Duration::from_secs(5))).expect("timeout");
                let mut bytes = Vec::new();
                let mut byte = [0];
                while !bytes.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).expect("headers");
                    bytes.push(byte[0]);
                }
                let headers = String::from_utf8(bytes).expect("utf8");
                let len = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                    })
                    .expect("length");
                let mut body = vec![0; len];
                socket.read_exact(&mut body).expect("body");
                let body: Value = serde_json::from_slice(&body).expect("json");
                let result = match step {
                    0 => {
                        assert!(headers.starts_with("PUT /collections/test/points?wait=true"));
                        assert_eq!(
                            body["points"][0]["payload"],
                            json!({"project_id":"p", "document_id":"d"})
                        );
                        json!({"result":true})
                    }
                    1 => {
                        assert_eq!(body["filter"]["must"][0]["match"]["value"], "d");
                        json!({"result":true})
                    }
                    _ => {
                        assert_eq!(body["filter"]["must"][0]["key"], "project_id");
                        assert_eq!(body["filter"]["must"][0]["match"]["value"], "p");
                        json!({"result":[{"id":"c", "score":0.9}]})
                    }
                }
                .to_string();
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}", result.len(), result).expect("response");
            }
        });
        let index = QdrantIndex {
            url: format!("http://{address}/collections/test"),
            key: String::new(),
            client: reqwest::Client::new(),
        };
        index
            .upsert(&[RagChunk {
                id: "c".into(),
                document_id: "d".into(),
                project_id: "p".into(),
                chunk_index: 0,
                heading: "private".into(),
                content: "private code".into(),
                embedding: vec![1.0, 0.0],
                created_at: 0,
            }])
            .await
            .expect("upsert");
        index.delete_document("d").await.expect("delete");
        assert_eq!(index.search("p", &[1.0, 0.0], 10).await.expect("search")[0].0, "c");
        worker.join().expect("worker");
    }
}
