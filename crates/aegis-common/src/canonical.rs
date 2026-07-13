//! Canonical JSON encoding — the byte layout everything is hashed and signed
//! over. Object keys sorted, no insignificant whitespace, stable float
//! formatting (serde_json's shortest-roundtrip). Deterministic across replays
//! by construction; a change here is a change to every hash in every log.

use serde::Serialize;
use serde_json::Value;

pub fn to_canonical_json<T: Serialize>(value: &T) -> Vec<u8> {
    let v = serde_json::to_value(value).expect("canonical encoding: serialization failed");
    let mut out = Vec::new();
    write_canonical(&v, &mut out);
    out
}

fn write_canonical(v: &Value, out: &mut Vec<u8>) {
    match v {
        Value::Object(map) => {
            out.push(b'{');
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                out.extend_from_slice(serde_json::to_string(k).unwrap().as_bytes());
                out.push(b':');
                write_canonical(&map[*k], out);
            }
            out.push(b'}');
        }
        Value::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_canonical(item, out);
            }
            out.push(b']');
        }
        other => out.extend_from_slice(serde_json::to_string(other).unwrap().as_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Sample {
        zebra: u32,
        alpha: &'static str,
        nested: Nested,
    }

    #[derive(Serialize)]
    struct Nested {
        b: bool,
        a: f64,
    }

    #[test]
    fn keys_are_sorted_and_output_is_stable() {
        let s = Sample {
            zebra: 1,
            alpha: "x",
            nested: Nested { b: true, a: 2.5 },
        };
        let bytes = to_canonical_json(&s);
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            r#"{"alpha":"x","nested":{"a":2.5,"b":true},"zebra":1}"#
        );
    }

    #[test]
    fn key_order_does_not_change_the_encoding() {
        let a = to_canonical_json(&serde_json::json!({"k": [1, 2, {"y": 1, "x": 2}]}));
        let b = to_canonical_json(&serde_json::json!({"k": [1, 2, {"x": 2, "y": 1}]}));
        assert_eq!(a, b);
        let c = to_canonical_json(&serde_json::json!({"k": [2, 1, {"y": 1, "x": 2}]}));
        assert_ne!(a, c); // array order IS significant
    }
}
