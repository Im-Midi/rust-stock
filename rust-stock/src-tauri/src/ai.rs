// ai.rs — AI Provider 抽象（OpenAI 兼容 Chat Completions 协议）
//
// 默认 DeepSeek。设置页改 base_url / model 即可切换任意兼容服务：
// OpenAI、Kimi(Moonshot)、通义、智谱、本地 Ollama(/v1) 等。命令层只管业务 prompt。

use serde_json::{json, Value};

pub const DEFAULT_BASE: &str = "https://api.deepseek.com";
pub const DEFAULT_MODEL: &str = "deepseek-chat";

#[derive(Debug, Clone)]
pub struct AiConfig {
    pub base_url: String,
    pub model: String,
    pub key: String,
}

impl AiConfig {
    /// 空串回退默认值；统一去掉 base_url 尾部斜杠
    pub fn new(key: String, base_url: Option<String>, model: Option<String>) -> Result<Self, String> {
        if key.trim().is_empty() {
            return Err("未配置 AI API Key（设置页）".into());
        }
        let base = base_url
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_BASE.into());
        let model = model
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.into());
        Ok(Self {
            base_url: base.trim().trim_end_matches('/').to_string(),
            model,
            key: key.trim().to_string(),
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }
}

/// 一次性问答（非流式），返回 content 文本
pub async fn chat_once(cfg: &AiConfig, messages: Vec<Value>, temperature: f64) -> Result<String, String> {
    let body = json!({ "model": cfg.model, "messages": messages, "temperature": temperature });
    let resp = reqwest::Client::new()
        .post(cfg.endpoint())
        .header("Authorization", format!("Bearer {}", cfg.key))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| format!("AI 请求失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("AI 服务返回 {status}: {text}"));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("解析响应失败: {e}"))?;
    v["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.trim().to_string())
        .ok_or_else(|| "AI 响应缺少 content".into())
}

/// 从回答里抠出第一个 JSON 对象（模型可能包 markdown 代码块）
pub fn extract_json(content: &str) -> Result<Value, String> {
    let start = content.find('{').ok_or("AI 未返回 JSON")?;
    let end = content.rfind('}').ok_or("AI 未返回 JSON")?;
    serde_json::from_str(&content[start..=end]).map_err(|e| format!("解析 AI JSON 失败: {e}"))
}

/// 流式问答：每个增量调一次 on_delta。SSE 分包可能不按行对齐，必须攒 buffer 按行切。
pub async fn chat_stream(
    cfg: &AiConfig,
    messages: Vec<Value>,
    temperature: f64,
    mut on_delta: impl FnMut(&str),
) -> Result<(), String> {
    use futures_util::StreamExt;
    let body = json!({ "model": cfg.model, "messages": messages, "stream": true, "temperature": temperature });
    let resp = reqwest::Client::new()
        .post(cfg.endpoint())
        .header("Authorization", format!("Bearer {}", cfg.key))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| format!("AI 请求失败: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("AI 服务返回 {status}: {text}"));
    }
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("流中断: {e}"))?;
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf.drain(..=pos);
            let Some(data) = line.strip_prefix("data:") else { continue };
            let data = data.trim();
            if data == "[DONE]" {
                return Ok(());
            }
            if let Ok(v) = serde_json::from_str::<Value>(data) {
                if let Some(delta) = v["choices"][0]["delta"]["content"].as_str() {
                    if !delta.is_empty() {
                        on_delta(delta);
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let c = AiConfig::new("sk-x".into(), None, None).unwrap();
        assert_eq!(c.base_url, DEFAULT_BASE);
        assert_eq!(c.model, DEFAULT_MODEL);
        assert!(AiConfig::new("  ".into(), None, None).is_err());
    }

    #[test]
    fn test_config_custom_provider() {
        let c = AiConfig::new(
            "sk-x".into(),
            Some("https://api.moonshot.cn/v1/".into()),
            Some("kimi-k2".into()),
        )
        .unwrap();
        assert_eq!(c.base_url, "https://api.moonshot.cn/v1"); // 尾部斜杠被去掉
        assert_eq!(c.endpoint(), "https://api.moonshot.cn/v1/chat/completions");
        assert_eq!(c.model, "kimi-k2");
    }

    #[test]
    fn test_extract_json() {
        let v = extract_json("```json\n{\"score\": -12, \"analysis\": \"理由\"}\n```").unwrap();
        assert_eq!(v["score"].as_i64(), Some(-12));
        assert!(extract_json("没有 json").is_err());
    }
}
