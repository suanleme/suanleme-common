//! Canonical JSON digests used to bind authorization contexts to business input.
//!
//! These helpers are shared by the gateway (PEP), the IAM PDP and business guards so a
//! digest computed before resolution and one recomputed at execution can be compared
//! byte-for-byte. They are **binding helpers, not signatures**: a digest proves neither
//! the sender's identity nor that the payload was already authorized.
//!
//! ## Canonical JSON (RFC 8785 / JCS subset)
//!
//! Implemented rules (one recursive encoder, no external canonicalization crate):
//!
//! * objects: keys sorted by **UTF-8 byte order** (`serde_json`'s default `Map` is a
//!   `BTreeMap` and is already byte-ordered; we sort explicitly so the rule survives a
//!   future `preserve_order` feature enablement);
//! * strings: JSON required escapes only — `"` `\` and controls `\b \t \n \f \r`,
//!   other C0 controls as lowercase `\u00XX`; non-ASCII characters stay unescaped;
//! * numbers: **integers only**, in the safe range `[-(2^53 - 1), 2^53 - 1]`, emitted as
//!   plain decimal (`-0` normalizes to `0`). Floating-point values — including integral
//!   floats such as `1.0` — and integers outside the safe range are rejected with the
//!   named error [`DIGEST_UNSUPPORTED_NUMBER`]; no rounding or truncation ever happens.
//!   `serde_json` cannot represent NaN/Infinity; if a caller ever hands one over it is
//!   rejected by the same rule (any non-integer number is unsupported);
//! * arrays: element order preserved;
//! * `null` / `true` / `false` as-is.
//!
//! Documented deviations from strict JCS: object duplicate keys cannot occur in
//! `serde_json::Value` (the parser rejects/deduplicates), and we do not implement the
//! JCS ES6 number serialization beyond safe integers — i.e. this is a strict subset,
//! not a full JCS implementation. Inputs containing unsupported numbers fail closed
//! rather than hashing an approximate representation.
//!
//! ## Digests
//!
//! * [`target_key`] = SHA-256 of canonical JSON
//!   `{provider, root_type, root_identity, target_role}` (64 lowercase hex chars);
//! * [`request_digest`] = SHA-256 of canonical JSON
//!   `{audience, subject, action, business_data}`;
//! * [`targets_digest`] = SHA-256 of the canonical JSON array of authorized targets
//!   **sorted by `target_key`** (ties broken deterministically by the canonical target
//!   payload). The digest binds every element and the set order is irrelevant; duplicate
//!   identical targets are **not** silently dropped — deduplication is the provider's
//!   contract (same role + same root ⇒ same `target_key`), and a service that recomputes
//!   the digest must apply the same rule.
//!
//! Raw non-JSON business data is not handled here: hash its original bytes with
//! [`sha256_hex`] instead (see `api-contract.md` §1.1).

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::authz::{AuthorizedTarget, RequestAction, RequestSubject};

/// Named error for floating-point / out-of-safe-range numbers. Never emitted for a
/// valid canonical integer.
pub const DIGEST_UNSUPPORTED_NUMBER: &str = "digest_unsupported_number";
/// Largest integer exactly representable by an IEEE-754 double (2^53 - 1).
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// SHA-256 over arbitrary bytes, lowercase hex (64 characters).
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        // Formatting into a String cannot fail.
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Canonicalize a JSON value according to the rules in the module documentation.
pub fn canonical_json(value: &Value) -> Result<String, &'static str> {
    let mut output = String::new();
    append_canonical(value, &mut output)?;
    Ok(output)
}

fn append_canonical(value: &Value, output: &mut String) -> Result<(), &'static str> {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(flag) => output.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => append_canonical_number(number, output)?,
        Value::String(text) => {
            let escaped =
                serde_json::to_string(text).map_err(|_| "digest_unsupported_string")?;
            output.push_str(&escaped);
        }
        Value::Array(items) => {
            output.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                append_canonical(item, output)?;
            }
            output.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
            output.push('{');
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                let escaped_key =
                    serde_json::to_string(key).map_err(|_| "digest_unsupported_string")?;
                output.push_str(&escaped_key);
                output.push(':');
                append_canonical(&map[key], output)?;
            }
            output.push('}');
        }
    }
    Ok(())
}

fn append_canonical_number(number: &serde_json::Number, output: &mut String) -> Result<(), &'static str> {
    if let Some(value) = number.as_i64() {
        if value < -MAX_SAFE_INTEGER || value > MAX_SAFE_INTEGER {
            return Err(DIGEST_UNSUPPORTED_NUMBER);
        }
        output.push_str(&value.to_string());
        return Ok(());
    }
    if let Some(value) = number.as_u64() {
        if value > MAX_SAFE_INTEGER as u64 {
            return Err(DIGEST_UNSUPPORTED_NUMBER);
        }
        output.push_str(&value.to_string());
        return Ok(());
    }
    // Any remaining representation is a floating-point value (e.g. `1.0`, `1.5`);
    // NaN/Infinity cannot be constructed through JSON parsing.
    Err(DIGEST_UNSUPPORTED_NUMBER)
}

/// Provider-generated target identity key:
/// `sha256(canonical_json({provider, root_type, root_identity, target_role}))`.
pub fn target_key(
    provider: &str,
    root_type: &str,
    root_identity: &str,
    target_role: &str,
) -> Result<String, &'static str> {
    if provider.trim().is_empty()
        || root_type.trim().is_empty()
        || root_identity.trim().is_empty()
        || target_role.trim().is_empty()
    {
        return Err("target_identity_empty");
    }
    let value = serde_json::json!({
        "provider": provider,
        "root_type": root_type,
        "root_identity": root_identity,
        "target_role": target_role,
    });
    Ok(sha256_hex(canonical_json(&value)?.as_bytes()))
}

/// The `target_key` carried by a serialized [`AuthorizedTarget`].
pub fn target_key_of(target: &AuthorizedTarget) -> &str {
    match target {
        AuthorizedTarget::Existing { target_key, .. }
        | AuthorizedTarget::NewRoot { target_key, .. } => target_key,
    }
}

/// SHA-256 of canonical JSON `{audience, subject, action, business_data}`.
///
/// `business_data` must be the business payload **before** any trusted envelope is
/// wrapped around it (post or get business projection), so the digest is stable across
/// formatting and does not cover tokens, `sign_str`, timestamps or the context itself.
pub fn request_digest(
    audience: &str,
    subject: &RequestSubject,
    action: &RequestAction,
    business_data: &Value,
) -> Result<String, &'static str> {
    let subject = serde_json::to_value(subject).map_err(|_| "digest_serialize_failed")?;
    let action = serde_json::to_value(action).map_err(|_| "digest_serialize_failed")?;
    let value = serde_json::json!({
        "audience": audience,
        "subject": subject,
        "action": action,
        "business_data": business_data,
    });
    Ok(sha256_hex(canonical_json(&value)?.as_bytes()))
}

/// SHA-256 of the canonical JSON array of `targets` sorted by `target_key`.
///
/// Order of the input slice is irrelevant. Equal `target_key`s are ordered by their
/// canonical payload only to keep the digest deterministic; they are preserved, not
/// deduplicated (see module docs).
pub fn targets_digest(targets: &[AuthorizedTarget]) -> Result<String, &'static str> {
    let mut entries: Vec<(String, String)> = Vec::with_capacity(targets.len());
    for target in targets {
        let key = target_key_of(target);
        if key.is_empty() {
            return Err("target_key_empty");
        }
        let serialized = serde_json::to_value(target)
            .map_err(|_| "digest_serialize_failed")
            .and_then(|value| canonical_json(&value))?;
        entries.push((key.to_string(), serialized));
    }
    entries.sort();
    let mut array = String::with_capacity(2 + entries.len() * 64);
    array.push('[');
    for (index, (_, serialized)) in entries.iter().enumerate() {
        if index > 0 {
            array.push(',');
        }
        array.push_str(serialized);
    }
    array.push(']');
    Ok(sha256_hex(array.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authz::UuidText;
    use serde_json::json;

    fn subject() -> RequestSubject {
        serde_json::from_value(json!({
            "tenant_id":"4","user_id":"7","tenant_type":"team"
        }))
        .unwrap()
    }

    fn action(path: &str) -> RequestAction {
        serde_json::from_value(json!({
            "method":"get","path":path,
            "route_contract_version":"deployment-project-transition-1"
        }))
        .unwrap()
    }

    fn target(key: &str, role: &str, object_id: &str) -> AuthorizedTarget {
        serde_json::from_value(json!({
            "target_kind":"existing",
            "target_key": key,
            "target_role": role,
            "action":{"method":"get","path":"/api/deployment/task/detail","route_contract_version":"deployment-project-transition-1"},
            "stamp":{
                "provider":"deployment","object_type":"task","object_id":object_id,"root_id":object_id,
                "mapping_version":"deployment-project-transition-1",
                "project":{"kind":"project","project_id":"16"},
                "resources":[],
                "physical_identities":[]
            }
        }))
        .unwrap()
    }

    fn new_root_target(key: &str, role: &str) -> AuthorizedTarget {
        serde_json::from_value(json!({
            "target_kind":"new_root",
            "target_key": key,
            "target_role": role,
            "action":{"method":"post","path":"/api/deployment/task/create","route_contract_version":"deployment-project-transition-1"},
            "new_root":{
                "provider":"deployment","object_type":"task",
                "project":{"kind":"project","project_id":"16"},
                "mapping_version":"deployment-project-transition-1",
                "resource_domain":"deployment-project-transition-1"
            }
        }))
        .unwrap()
    }

    #[test]
    fn sha256_hex_matches_known_vector_and_is_lowercase_hex() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        for input in [b"".as_slice(), b"a", b"hello world"] {
            let hex = sha256_hex(input);
            assert_eq!(hex.len(), 64);
            assert!(hex.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
        }
    }

    #[test]
    fn canonical_json_sorts_object_keys_by_utf8_bytes_and_keeps_array_order() {
        let value = json!({
            "b": 1,
            "a": {"d":"x","c":[2,1]},
            "Z": null,
            "é": true
        });
        assert_eq!(
            canonical_json(&value).unwrap(),
            r#"{"Z":null,"a":{"c":[2,1],"d":"x"},"b":1,"é":true}"#
        );
        assert_ne!(
            canonical_json(&json!([1, 2])).unwrap(),
            canonical_json(&json!([2, 1])).unwrap()
        );
        assert_eq!(
            canonical_json(&json!({"nested":{"b":2,"a":1}})).unwrap(),
            r#"{"nested":{"a":1,"b":2}}"#
        );
    }

    #[test]
    fn canonical_json_escapes_strings_minimally() {
        let value = json!({"s": "a\"b\\c\u{0008}\u{001f}é"});
        // Expected: quote/backslash escaped, \b short escape, other C0 as lowercase
        // \u00XX, non-ASCII kept raw.
        let expected = "{\"s\":\"a\\\"b\\\\c\\b\\u001fé\"}";
        assert_eq!(canonical_json(&value).unwrap(), expected);
        assert_eq!(canonical_json(&json!("\u{0000}")).unwrap(), r#""\u0000""#);
    }

    #[test]
    fn canonical_json_rejects_float_and_unsafe_numbers_with_named_error() {
        for unsupported in [
            json!(1.5),
            json!(1.0),
            json!(-1.0),
            json!(0.0),
            json!(9_007_199_254_740_992_i64),
            json!(-9_007_199_254_740_992_i64),
            json!(u64::MAX),
        ] {
            assert_eq!(
                canonical_json(&json!({"n": unsupported})),
                Err(DIGEST_UNSUPPORTED_NUMBER),
                "{unsupported}"
            );
        }
        for supported in [
            json!(0),
            json!(-1),
            json!(9_007_199_254_740_991_i64),
            json!(-9_007_199_254_740_991_i64),
        ] {
            assert!(canonical_json(&json!({"n": supported})).is_ok(), "{supported}");
        }
        assert_eq!(canonical_json(&json!({"n": 0})).unwrap(), r#"{"n":0}"#);
        // A digest never rounds an unsupported nested number into a string or float.
        assert_eq!(
            request_digest(
                "gj-deployment-api",
                &subject(),
                &action("/api/deployment/task/detail"),
                &json!({"task_id": 1.5})
            ),
            Err(DIGEST_UNSUPPORTED_NUMBER)
        );
    }

    #[test]
    fn target_key_is_stable_and_discriminates_every_component() {
        let key = target_key("deployment", "task", "123", "primary").unwrap();
        assert_eq!(key.len(), 64);
        assert_eq!(
            key,
            target_key("deployment", "task", "123", "primary").unwrap()
        );
        for other in [
            target_key("deployment", "task", "124", "primary"),
            target_key("deployment", "task", "123", "secondary"),
            target_key("deployment", "pod", "123", "primary"),
            target_key("storage", "task", "123", "primary"),
        ] {
            assert_ne!(key, other.unwrap());
        }
        // Different target kinds for the same role+identity share the key: identity is
        // the provider tuple, not the DTO variant.
        let existing = target(&key, "primary", "123");
        let created = new_root_target(&key, "primary");
        assert_eq!(target_key_of(&existing), target_key_of(&created));
        // Same role and root but different object payload still maps to the same key.
        assert_eq!(
            target_key_of(&target(&key, "primary", "999")),
            key.as_str()
        );
        assert_eq!(target_key("", "task", "1", "primary"), Err("target_identity_empty"));
        assert_eq!(target_key("deployment", " ", "1", "primary"), Err("target_identity_empty"));
    }

    #[test]
    fn request_digest_binds_audience_subject_action_and_business_data() {
        let business = json!({"task_id":"123","page":"1","page_size":"20"});
        let base = request_digest(
            "gj-deployment-api",
            &subject(),
            &action("/api/deployment/task/detail"),
            &business,
        )
        .unwrap();
        assert_eq!(base.len(), 64);
        // Object key order is canonicalized away.
        let reordered = json!({"page_size":"20","task_id":"123","page":"1"});
        assert_eq!(
            base,
            request_digest(
                "gj-deployment-api",
                &subject(),
                &action("/api/deployment/task/detail"),
                &reordered
            )
            .unwrap()
        );
        // Every bound component changes the digest.
        assert_ne!(
            base,
            request_digest("gj-storage-api", &subject(), &action("/api/deployment/task/detail"), &business).unwrap()
        );
        let other_subject: RequestSubject =
            serde_json::from_value(json!({"tenant_id":"4","user_id":"8","tenant_type":"team"}))
                .unwrap();
        assert_ne!(
            base,
            request_digest("gj-deployment-api", &other_subject, &action("/api/deployment/task/detail"), &business).unwrap()
        );
        assert_ne!(
            base,
            request_digest("gj-deployment-api", &subject(), &action("/api/deployment/task/search"), &business).unwrap()
        );
        assert_ne!(
            base,
            request_digest(
                "gj-deployment-api",
                &subject(),
                &action("/api/deployment/task/detail"),
                &json!({"task_id":"124"})
            )
            .unwrap()
        );
        // Array element order is meaningful for business data and changes the digest.
        assert_ne!(
            request_digest("a", &subject(), &action("/p"), &json!({"ids":[1,2]})).unwrap(),
            request_digest("a", &subject(), &action("/p"), &json!({"ids":[2,1]})).unwrap()
        );
    }

    #[test]
    fn targets_digest_is_order_insensitive_and_deterministic() {
        let first = target("a".repeat(64).as_str(), "primary", "123");
        let second = target("b".repeat(64).as_str(), "secondary", "456");
        let one = targets_digest(&[first.clone(), second.clone()]).unwrap();
        let two = targets_digest(&[second.clone(), first.clone()]).unwrap();
        assert_eq!(one, two);
        assert_eq!(one.len(), 64);
        // Adding, removing or changing a target changes the digest.
        assert_ne!(one, targets_digest(&[first.clone()]).unwrap());
        assert_ne!(
            one,
            targets_digest(&[first.clone(), target("c".repeat(64).as_str(), "secondary", "456")])
                .unwrap()
        );
        assert!(targets_digest(&[]).is_ok());
        // Duplicate identical targets are preserved (not silently deduplicated) but the
        // result stays deterministic for any input order.
        let duplicate = targets_digest(&[first.clone(), first.clone()]).unwrap();
        assert_ne!(duplicate, targets_digest(&[first.clone()]).unwrap());
        assert_eq!(
            duplicate,
            targets_digest(&[first.clone(), first.clone()]).unwrap()
        );
        // Same target_key with different payload gets a deterministic tie-break.
        let same_key_a = target("d".repeat(64).as_str(), "primary", "1");
        let same_key_b = target("d".repeat(64).as_str(), "primary", "2");
        assert_eq!(
            targets_digest(&[same_key_a.clone(), same_key_b.clone()]).unwrap(),
            targets_digest(&[same_key_b, same_key_a]).unwrap()
        );
        // UuidText is exercised through a target action-independent digest path.
        assert!(UuidText::new("not-a-uuid").is_err());
    }
}
