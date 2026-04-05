//! stdio JSON-RPC 2.0 server for MCP protocol.

use anyhow::Result;
use serde_json::json;
use std::io::{self, BufRead, Write};

use super::tools;

pub async fn run() -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }

        let request: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let id = request.get("id").cloned();
        let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("");

        let result = match method {
            "initialize" => handle_initialize(),
            "notifications/initialized" => continue,
            "tools/list" => handle_tools_list(),
            "tools/call" => {
                let params = request.get("params").cloned().unwrap_or(json!({}));
                tools::handle_tool_call(params).await
            }
            _ => {
                json!({ "error": { "code": -32601, "message": format!("Unknown method: {method}") } })
            }
        };

        let response = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        });

        let mut out = stdout.lock();
        writeln!(out, "{}", serde_json::to_string(&response)?)?;
        out.flush()?;
    }

    Ok(())
}

fn handle_initialize() -> serde_json::Value {
    json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "korea-cli",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

fn handle_tools_list() -> serde_json::Value {
    json!({
        "tools": [
            {
                "name": "search_api",
                "description": "한국 공공데이터포털 API 카탈로그를 검색합니다. 키워드, 카테고리로 후보 API를 찾습니다. 결과의 spec_status가 Available 또는 PartialStub인 API(is_callable=true)만 get_api_spec/call_api로 사용 가능합니다. PartialStub은 일부 operation만 수집된 상태입니다.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "검색 키워드" },
                        "category": { "type": "string", "description": "카테고리 필터 (선택)" },
                        "limit": { "type": "number", "description": "결과 수 (기본 10)" }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "get_api_spec",
                "description": "특정 API의 상세 스펙(파라미터, 응답 스키마)을 조회합니다. search_api로 찾은 list_id를 사용하세요.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "list_id": { "type": "string", "description": "API 서비스 ID" }
                    },
                    "required": ["list_id"]
                }
            },
            {
                "name": "call_api",
                "description": "API를 호출하고 결과를 반환합니다. get_api_spec으로 파라미터를 확인한 후 사용하세요.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "list_id": { "type": "string", "description": "API 서비스 ID" },
                        "operation": { "type": "string", "description": "오퍼레이션 경로 (예: /status)" },
                        "params": { "type": "object", "description": "API 파라미터" }
                    },
                    "required": ["list_id", "operation"]
                }
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_initialize() {
        let result = handle_initialize();
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert!(result["capabilities"]["tools"].is_object());
        assert_eq!(result["serverInfo"]["name"], "korea-cli");
    }

    #[test]
    fn test_handle_tools_list() {
        let result = handle_tools_list();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 3);

        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"search_api"));
        assert!(names.contains(&"get_api_spec"));
        assert!(names.contains(&"call_api"));

        // Verify each tool has inputSchema with required fields
        for tool in tools {
            assert!(tool["inputSchema"].is_object());
            assert!(tool["inputSchema"]["required"].is_array());
        }
    }
}
