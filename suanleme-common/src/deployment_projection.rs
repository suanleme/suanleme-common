//! Single shared implementation of the **GET business-query projection** for the
//! deployment task routes.
//!
//! The gateway hashes the business input into `request_digest` and the business guard
//! recomputes the same digest over the actual request; if the two sides normalized the
//! query differently the digest would only match by accident. This module freezes one
//! projection per registered route so both sides call the same function:
//!
//! * the gateway (after stripping client-supplied trusted fields) calls it on the real
//!   query parameter pairs;
//! * the deployment guard calls it on the query pairs it actually parses.
//!
//! Rules:
//!
//! * `route_key` is the canonical `"GET <path>"` string (uppercase method, path without
//!   query); any other method/path is rejected as unsupported — no prefix guessing;
//! * only the fields listed for that route are business input. **Unknown fields are
//!   rejected** (`unknown_business_parameter`) rather than ignored: this also means a
//!   trusted field that failed to be stripped fails closed here instead of being bound
//!   into the digest;
//! * `task_id`, `page`, `page_size` are integers: the raw value must be ASCII decimal
//!   digits (`+`/`-`/spaces/empty are rejected), is parsed as `i64` (overflow rejected)
//!   and rendered as the canonical decimal string (`"007"` -> `"7"`, no sign, no
//!   leading zeros);
//! * `merchant_mark`, `type`, `status`, `search_value`, `start_time`, `end_time` are
//!   opaque text values kept byte-for-byte; an empty value is rejected;
//! * required fields: `task_id` on detail / `get_replicas_record` / `points_count`;
//!   everything else is optional and an absent optional field is absent from the output
//!   (not `null`, not empty string);
//! * repeated parameters are rejected (`duplicate_business_parameter`) so the gateway
//!   and the handler can never interpret the same query differently;
//! * the output is a JSON object of strings only, sorted by key through
//!   `serde_json::Map` (BTreeMap) / canonical serialization.
//!
//! Nothing here queries SQL, IAM or the database; it is a pure normalization function.

use serde_json::{Map, Value};

#[derive(Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    /// Free text kept verbatim; empty is invalid.
    Text,
    /// ASCII decimal integer canonicalized through `i64`.
    Integer,
    /// Strict boolean literal (`true`/`false` only); canonicalized to lower-case text.
    Bool,
}

#[derive(Clone, Copy)]
struct FieldSpec {
    name: &'static str,
    kind: FieldKind,
    required: bool,
}

const fn text(name: &'static str, required: bool) -> FieldSpec {
    FieldSpec {
        name,
        kind: FieldKind::Text,
        required,
    }
}

const fn integer(name: &'static str, required: bool) -> FieldSpec {
    FieldSpec {
        name,
        kind: FieldKind::Integer,
        required,
    }
}

const fn boolean(name: &'static str, required: bool) -> FieldSpec {
    FieldSpec {
        name,
        kind: FieldKind::Bool,
        required,
    }
}

const DETAIL_FIELDS: &[FieldSpec] = &[integer("task_id", true)];
const SEARCH_FIELDS: &[FieldSpec] = &[
    text("merchant_mark", false),
    text("type", false),
    text("status", false),
    text("search_value", false),
    integer("page", false),
    integer("page_size", false),
];
const REPLICAS_RECORD_FIELDS: &[FieldSpec] = &[
    integer("task_id", true),
    text("start_time", false),
    text("end_time", false),
    integer("page", false),
    integer("page_size", false),
];
const POINTS_COUNT_FIELDS: &[FieldSpec] = &[
    integer("task_id", true),
    text("merchant_mark", false),
];
/// DistributedTraining（REQ-08 R4）：训练任务列表的真实 query 字段（与 controller 参数同名）。
const TRAINING_SEARCH_FIELDS: &[FieldSpec] = &[
    text("merchant_mark", false),
    text("status", false),
    text("sub_type", false),
    text("schedule_status", false),
    text("search_value", false),
    boolean("group_task", false),
    integer("group_id", false),
    text("framework", false),
    integer("page", false),
    integer("page_size", false),
];
/// DistributedTraining（REQ-08 R4）：训练任务详情只按真实 task_id 绑定。
const TRAINING_DETAIL_FIELDS: &[FieldSpec] = &[
    integer("task_id", true),
    text("merchant_mark", false),
];
/// 实例列表（R4 接入）：task 根 + 分页/状态/地域过滤。
const POINTS_FIELDS: &[FieldSpec] = &[
    text("merchant_mark", false),
    integer("task_id", true),
    integer("page", false),
    integer("page_size", false),
    text("status", false),
    text("regions", false),
];
/// 实例日志（R4 接入）：task 根 + Pod/实例定位与容器/previous 过滤。
const POINT_LOG_FIELDS: &[FieldSpec] = &[
    integer("task_id", true),
    integer("pod_id", false),
    integer("point_id", false),
    integer("service_id", false),
    text("container_name", false),
    boolean("previous", false),
];
/// Pod 事件（R4 接入）：Pod 根（point_id），归属经 task 继承。
const POD_EVENT_FIELDS: &[FieldSpec] = &[integer("point_id", true)];
/// 任务事件 v2（R4 接入）：task 根。
const TASK_EVENT_V2_FIELDS: &[FieldSpec] = &[
    text("merchant_mark", false),
    integer("task_id", true),
];

fn fields_for_path(path: &str) -> Option<&'static [FieldSpec]> {
    match path {
        "/api/deployment/task/detail" | "/api/task/deployment/detail" => Some(DETAIL_FIELDS),
        "/api/deployment/task/search" | "/api/task/deployment/search" => Some(SEARCH_FIELDS),
        "/api/deployment/task/get_replicas_record" => Some(REPLICAS_RECORD_FIELDS),
        "/api/deployment/task/points_count" => Some(POINTS_COUNT_FIELDS),
        "/api/task/distributed-training/search" => Some(TRAINING_SEARCH_FIELDS),
        "/api/task/distributed-training/detail" => Some(TRAINING_DETAIL_FIELDS),
        "/api/deployment/task/points" => Some(POINTS_FIELDS),
        "/api/deployment/task/point_log" => Some(POINT_LOG_FIELDS),
        "/api/deployment/task/pod_event" => Some(POD_EVENT_FIELDS),
        "/api/deployment/task/task_event_v2" => Some(TASK_EVENT_V2_FIELDS),
        _ => None,
    }
}

/// Project the GET query of a registered deployment route into the frozen business-data
/// JSON object used for digest binding. See the module documentation for the rules.
pub fn business_query_projection(
    route_key: &str,
    params: &[(String, String)],
) -> Result<Value, &'static str> {
    let (method, path) = route_key
        .split_once(' ')
        .ok_or("unsupported_projection_route")?;
    if method != "GET" {
        return Err("unsupported_projection_route");
    }
    let fields = fields_for_path(path).ok_or("unsupported_projection_route")?;

    // Reject unknown and duplicate parameters before building anything.
    let mut seen: Vec<&str> = Vec::with_capacity(params.len());
    for (name, _) in params {
        if !fields.iter().any(|spec| spec.name == name) {
            return Err("unknown_business_parameter");
        }
        if seen.contains(&name.as_str()) {
            return Err("duplicate_business_parameter");
        }
        seen.push(name);
    }

    let mut projected = Map::new();
    for spec in fields {
        let Some((_, raw)) = params.iter().find(|(name, _)| name == spec.name) else {
            if spec.required {
                return Err("missing_business_parameter");
            }
            continue;
        };
        let rendered = match spec.kind {
            FieldKind::Text => {
                if raw.is_empty() {
                    return Err("invalid_business_parameter");
                }
                raw.clone()
            }
            FieldKind::Integer => canonical_integer(raw)?,
            FieldKind::Bool => canonical_bool(raw)?,
        };
        projected.insert(spec.name.to_string(), Value::String(rendered));
    }
    Ok(Value::Object(projected))
}

fn canonical_bool(raw: &str) -> Result<String, &'static str> {
    match raw {
        "true" => Ok("true".to_owned()),
        "false" => Ok("false".to_owned()),
        _ => Err("invalid_business_parameter"),
    }
}

fn canonical_integer(raw: &str) -> Result<String, &'static str> {
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("invalid_business_parameter");
    }
    let parsed: i64 = raw.parse().map_err(|_| "invalid_business_parameter")?;
    Ok(parsed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn params(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect()
    }

    #[test]
    fn detail_routes_project_a_canonical_task_id() {
        for route in [
            "GET /api/deployment/task/detail",
            "GET /api/task/deployment/detail",
        ] {
            assert_eq!(
                business_query_projection(route, &params(&[("task_id", "123")])).unwrap(),
                json!({"task_id":"123"})
            );
            // Digit-only values are parsed and re-rendered canonically.
            assert_eq!(
                business_query_projection(route, &params(&[("task_id", "000123")])).unwrap(),
                json!({"task_id":"123"})
            );
            // Optional fields are absent rather than null/empty.
            assert_eq!(
                business_query_projection(route, &params(&[])),
                Err("missing_business_parameter")
            );
        }
    }

    #[test]
    fn search_routes_accept_exactly_the_frozen_field_set() {
        let input = params(&[
            ("merchant_mark", "gj-team"),
            ("type", "normal"),
            ("status", "RUNNING"),
            ("search_value", "  name with space  "),
            ("page", "02"),
            ("page_size", "20"),
        ]);
        let expected = json!({
            "merchant_mark":"gj-team","type":"normal","status":"RUNNING",
            "search_value":"  name with space  ","page":"2","page_size":"20"
        });
        for route in [
            "GET /api/deployment/task/search",
            "GET /api/task/deployment/search",
        ] {
            assert_eq!(
                business_query_projection(route, &input).unwrap(),
                expected
            );
        }
        // Query object key order is canonical (serde_json Map = BTreeMap).
        let rendered =
            serde_json::to_string(&business_query_projection("GET /api/deployment/task/search", &input).unwrap())
                .unwrap();
        assert_eq!(
            rendered,
            r#"{"merchant_mark":"gj-team","page":"2","page_size":"20","search_value":"  name with space  ","status":"RUNNING","type":"normal"}"#
        );
        // Every field is optional on the search routes.
        assert_eq!(
            business_query_projection("GET /api/deployment/task/search", &[]).unwrap(),
            json!({})
        );
    }

    #[test]
    fn replicas_record_and_points_count_routes_project_their_own_fields() {
        assert_eq!(
            business_query_projection(
                "GET /api/deployment/task/get_replicas_record",
                &params(&[
                    ("task_id", "9"),
                    ("start_time", "2026-10-19 00:00:00"),
                    ("end_time", "2026-10-19 23:59:59"),
                    ("page", "1"),
                    ("page_size", "50"),
                ])
            )
            .unwrap(),
            json!({
                "task_id":"9","start_time":"2026-10-19 00:00:00",
                "end_time":"2026-10-19 23:59:59","page":"1","page_size":"50"
            })
        );
        assert_eq!(
            business_query_projection(
                "GET /api/deployment/task/points_count",
                &params(&[("task_id", "9"), ("merchant_mark", "m")])
            )
            .unwrap(),
            json!({"task_id":"9","merchant_mark":"m"})
        );
        // A field frozen for another route is still unknown on this one.
        assert_eq!(
            business_query_projection(
                "GET /api/deployment/task/points_count",
                &params(&[("task_id", "9"), ("page", "1")])
            ),
            Err("unknown_business_parameter")
        );
    }

    #[test]
    fn unknown_stripped_fields_fail_closed() {
        for (route, field) in [
            ("GET /api/deployment/task/detail", "authorization_context"),
            ("GET /api/deployment/task/search", "authorization_context"),
            ("GET /api/deployment/task/search", "tenant_id"),
            ("GET /api/deployment/task/search", "resource_group_id"),
            ("GET /api/deployment/task/search", "user_id"),
        ] {
            assert_eq!(
                business_query_projection(route, &params(&[(field, "x")])),
                Err("unknown_business_parameter"),
                "{route} {field}"
            );
        }
    }

    #[test]
    fn invalid_and_duplicate_values_are_rejected() {
        let detail = "GET /api/deployment/task/detail";
        for value in [
            "", " 123", "123 ", "-1", "+1", "1.0", "1e3", "abc", "9223372036854775808",
        ] {
            assert_eq!(
                business_query_projection(detail, &params(&[("task_id", value)])),
                Err("invalid_business_parameter"),
                "{value:?}"
            );
        }
        assert_eq!(
            business_query_projection(
                "GET /api/deployment/task/search",
                &params(&[("merchant_mark", "")])
            ),
            Err("invalid_business_parameter")
        );
        assert_eq!(
            business_query_projection(detail, &params(&[("task_id", "1"), ("task_id", "1")])),
            Err("duplicate_business_parameter")
        );
        assert_eq!(
            business_query_projection(
                "GET /api/deployment/task/search",
                &params(&[("page", "1"), ("page", "2")])
            ),
            Err("duplicate_business_parameter")
        );
    }

    #[test]
    fn unsupported_routes_and_methods_are_rejected() {
        for route in [
            "GET /api/deployment/task/update",
            "GET /api/deployment/task/detail/",
            "POST /api/deployment/task/search",
            "get /api/deployment/task/search",
            "GET /api/task/deployment/search?page=1",
            "/api/deployment/task/search",
            "",
        ] {
            assert_eq!(
                business_query_projection(route, &[]),
                Err("unsupported_projection_route"),
                "{route}"
            );
        }
    }

    #[test]
    fn distributed_training_routes_project_their_real_query_fields() {
        assert_eq!(
            business_query_projection(
                "GET /api/task/distributed-training/detail",
                &params(&[("task_id", "007")])
            )
            .unwrap(),
            json!({"task_id":"7"})
        );
        assert_eq!(
            business_query_projection(
                "GET /api/task/distributed-training/search",
                &params(&[
                    ("status", "Running"),
                    ("group_task", "true"),
                    ("group_id", "42"),
                    ("framework", "PyTorch"),
                    ("page", "1"),
                    ("page_size", "20"),
                ])
            )
            .unwrap(),
            json!({
                "status":"Running","group_task":"true","group_id":"42",
                "framework":"PyTorch","page":"1","page_size":"20"
            })
        );
        // 布尔只接受严格 true/false（与框架查询参数解析一致），不做宽松转换。
        assert_eq!(
            business_query_projection(
                "GET /api/task/distributed-training/search",
                &params(&[("group_task", "True")])
            ),
            Err("invalid_business_parameter")
        );
        // 未知字段仍然 fail-closed（不得静默忽略进摘要）。
        assert_eq!(
            business_query_projection(
                "GET /api/task/distributed-training/search",
                &params(&[("project_id", "16")])
            ),
            Err("unknown_business_parameter")
        );
    }

    #[test]
    fn projection_values_are_strings_not_numbers() {
        let value =
            business_query_projection("GET /api/deployment/task/detail", &params(&[("task_id", "7")]))
                .unwrap();
        assert!(value.get("task_id").unwrap().is_string());
        assert!(!value.get("task_id").unwrap().is_number());
        // Text values keep non-ASCII/verbatim content.
        assert_eq!(
            business_query_projection(
                "GET /api/deployment/task/search",
                &params(&[("search_value", "任务 é")])
            )
            .unwrap(),
            json!({"search_value":"任务 é"})
        );
    }
}
