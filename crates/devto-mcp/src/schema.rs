//! Hold a call to the schema its tool publishes.
//!
//! Every tool ships an `inputSchema` saying which arguments it takes, which values are legal
//! and which are required. Until this existed, nothing checked a call against it: a misspelled
//! argument was dropped and an out-of-range enum fell through to a default, so
//! `my_articles {"status": "nonsense"}` quietly returned *every* article and
//! `my_articles {"stauts": "published"}` quietly returned drafts too. A wrong answer that
//! looks right is the worst outcome available to a tool whose whole purpose is catching
//! mistakes before they cost a request.
//!
//! This is not a JSON Schema implementation and should not become one. It covers exactly the
//! keywords the tools actually declare — `type`, `properties`, `required`,
//! `additionalProperties`, `enum`, `minimum`, `maximum` and `items` — and a test fails if a
//! schema starts using something it does not enforce, because silently ignoring a constraint
//! is the bug this module exists to fix.

use serde_json::Value;

/// Keywords this validator understands. A schema may only use these.
///
/// Only a test reads it — `no_schema_uses_a_keyword_the_validator_ignores` — which is the
/// point: it is the list the tools are held to, not something the validator consults at run
/// time. `check` matches on the keywords directly.
#[cfg(test)]
pub const SUPPORTED: &[&str] = &[
    "$schema",
    "description",
    "type",
    "properties",
    "required", // recognised, but left to the tools — see the note in `check`
    "additionalProperties",
    "enum",
    "minimum",
    "maximum",
    "items",
    "x-refused",
];

/// Check `args` against `schema`, returning every problem rather than the first.
///
/// Reporting all of them matters: a caller that fixes one argument per round trip spends the
/// rate budget learning what it could have been told once.
pub fn validate(schema: &Value, args: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    check(schema, args, "arguments", &mut problems);
    problems
}

fn check(schema: &Value, value: &Value, path: &str, problems: &mut Vec<String>) {
    if let Some(expected) = schema.get("type").and_then(Value::as_str)
        && !type_matches(expected, value)
    {
        problems.push(format!(
            "{path} should be {expected}, got {}.",
            describe(value)
        ));
        // Every further check assumes the type, so stop before producing noise about it.
        return;
    }

    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.contains(value)
    {
        let list: Vec<String> = allowed.iter().map(render).collect();
        problems.push(format!(
            "{path} is {} — it has to be one of {}.",
            render(value),
            list.join(", ")
        ));
    }

    if let Some(number) = value.as_f64() {
        if let Some(min) = schema.get("minimum").and_then(Value::as_f64)
            && number < min
        {
            problems.push(format!(
                "{path} is {}, and the minimum is {min}.",
                render(value)
            ));
        }
        if let Some(max) = schema.get("maximum").and_then(Value::as_f64)
            && number > max
        {
            problems.push(format!(
                "{path} is {}, and the maximum is {max}.",
                render(value)
            ));
        }
    }

    if let Some(object) = value.as_object() {
        let properties = schema.get("properties").and_then(Value::as_object);

        // `required` is deliberately not enforced here. The tools already refuse a missing
        // argument themselves, and their messages are better than anything this could say:
        // omitting `ai_disclosure_level` gets back the three values it accepts and why the
        // choice is the caller's, where a generic "is required" would send them to the schema
        // to find out. A check here would only pre-empt that with something worse.

        // `additionalProperties: false` is the one that catches a typo, which is the most
        // common way a call goes wrong and the least likely to be noticed.
        if schema.get("additionalProperties") == Some(&Value::Bool(false))
            && let Some(properties) = properties
        {
            for key in object.keys() {
                if !properties.contains_key(key) {
                    // A tool that refuses a particular argument on purpose says why in
                    // `x-refused`, and its own wording is better than anything generic: the
                    // useful reply to `update_article {"published": true}` is not "no such
                    // argument", it is "publication state is changed with publish_article".
                    if let Some(reason) = schema
                        .get("x-refused")
                        .and_then(|r| r.get(key))
                        .and_then(Value::as_str)
                    {
                        problems.push(format!("{path}.{key} is not accepted here. {reason}"));
                        continue;
                    }

                    let mut message = format!("{path}.{key} is not an argument of this tool.");
                    if let Some(near) = nearest(key, properties.keys().map(String::as_str)) {
                        message.push_str(&format!(" Did you mean {near}?"));
                    } else {
                        let known: Vec<&str> = properties.keys().map(String::as_str).collect();
                        message.push_str(&format!(" It takes: {}.", known.join(", ")));
                    }
                    problems.push(message);
                }
            }
        }

        if let Some(properties) = properties {
            for (key, item) in object {
                if let Some(sub) = properties.get(key) {
                    check(sub, item, &format!("{path}.{key}"), problems);
                }
            }
        }
    }

    if let Some(array) = value.as_array()
        && let Some(items) = schema.get("items")
    {
        for (i, item) in array.iter().enumerate() {
            check(items, item, &format!("{path}[{i}]"), problems);
        }
    }
}

fn type_matches(expected: &str, value: &Value) -> bool {
    match expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        // JSON has one number type; "integer" additionally rules out a fractional part.
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn describe(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        // `is_f64()` alone: serde_json never gives an `i64` back for a float, so the
        // `as_i64().is_none()` this used to also test was always true here and decided
        // nothing. It did change one case — a `u64` above `i64::MAX` is not a float, and
        // calling it fractional would be wrong.
        Value::Number(n) if n.is_f64() => "a fractional number",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

fn render(value: &Value) -> String {
    match value {
        Value::String(s) => format!("\"{s}\""),
        other => other.to_string(),
    }
}

/// The closest known argument name, when one is close enough to be worth suggesting.
///
/// A typo is the case this whole module is for, and "did you mean `status`?" is the
/// difference between one round trip and several.
fn nearest<'a>(given: &str, known: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    known
        .map(|candidate| (distance(given, candidate), candidate))
        // Two edits on a short name is already a stretch; beyond that a suggestion misleads.
        .filter(|(d, candidate)| *d <= 2.min(candidate.len().div_ceil(3)).max(1))
        .min_by_key(|(d, _)| *d)
        .map(|(_, candidate)| candidate)
}

/// Levenshtein distance, two rows at a time.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];

    for (i, ca) in a.chars().enumerate() {
        current[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != cb);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "status": {"type": "string", "enum": ["published", "unpublished", "all"]},
                "page": {"type": "integer", "minimum": 1},
                "per_page": {"type": "integer", "minimum": 1, "maximum": 100},
                "tags": {"type": "array", "items": {"type": "string"}}
            },
            "additionalProperties": false
        })
    }

    #[test]
    fn a_correct_call_has_nothing_to_say() {
        let problems = validate(&schema(), &json!({"status": "published", "per_page": 3}));
        assert!(problems.is_empty(), "{problems:?}");
        assert!(
            validate(&schema(), &json!({})).is_empty(),
            "no arguments is fine"
        );
    }

    /// The call that started this: `status` outside its enum used to fall through to a
    /// default and return every article, which reads as a successful answer.
    #[test]
    fn a_value_outside_its_enum_is_refused() {
        let problems = validate(&schema(), &json!({"status": "nonsense"}));
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("\"nonsense\""), "{}", problems[0]);
        assert!(
            problems[0].contains("published"),
            "it should list what is allowed"
        );
    }

    /// The other half: a misspelled argument was dropped in silence.
    #[test]
    fn an_unknown_argument_is_refused_and_a_near_miss_is_named() {
        let problems = validate(&schema(), &json!({"stauts": "published"}));
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(
            problems[0].contains("Did you mean status?"),
            "{}",
            problems[0]
        );

        // Nothing close enough to guess at: say what the tool does take instead.
        let problems = validate(&schema(), &json!({"totally_bogus_arg": "xyz"}));
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(!problems[0].contains("Did you mean"), "{}", problems[0]);
        assert!(problems[0].contains("It takes:"), "{}", problems[0]);
    }

    /// An argument refused on purpose gets the tool's own explanation, not a generic one.
    #[test]
    fn a_deliberately_refused_argument_explains_itself() {
        let s = json!({
            "type": "object",
            "properties": {"article_id": {"type": "integer"}},
            "additionalProperties": false,
            "x-refused": {
                "published": "Use publish_article or unpublish_article, which carry their own permissions."
            }
        });
        let problems = validate(&s, &json!({"article_id": 7, "published": true}));
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("publish_article"), "{}", problems[0]);
        assert!(!problems[0].contains("It takes:"), "{}", problems[0]);
    }

    #[test]
    fn numbers_are_held_to_their_bounds() {
        assert!(validate(&schema(), &json!({"page": 0}))[0].contains("minimum is 1"));
        assert!(validate(&schema(), &json!({"per_page": 500}))[0].contains("maximum is 100"));
        assert!(validate(&schema(), &json!({"per_page": 100})).is_empty());
    }

    #[test]
    fn a_wrong_type_is_named_rather_than_coerced() {
        let problems = validate(&schema(), &json!({"page": "two"}));
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("should be integer"), "{}", problems[0]);

        // A fractional number is not an integer, and saying so beats rounding silently.
        let problems = validate(&schema(), &json!({"page": 1.5}));
        assert!(problems[0].contains("fractional"), "{}", problems[0]);
    }

    /// The bounds are inclusive, and a value sitting exactly on one is the case a `<` and a
    /// `<=` disagree about — which is the only case worth writing down.
    #[test]
    fn a_value_exactly_on_a_bound_is_allowed() {
        assert!(
            validate(&schema(), &json!({"page": 1})).is_empty(),
            "minimum is inclusive"
        );
        assert!(
            validate(&schema(), &json!({"per_page": 100})).is_empty(),
            "maximum is inclusive"
        );
        assert!(!validate(&schema(), &json!({"page": 0})).is_empty());
        assert!(!validate(&schema(), &json!({"per_page": 101})).is_empty());
    }

    /// Every type the tools declare, checked both ways. Without this a deleted arm falls
    /// through to "anything matches" and the schema stops meaning anything.
    #[test]
    fn each_declared_type_accepts_only_itself() {
        let cases: [(&str, Value, Value); 6] = [
            ("string", json!("x"), json!(1)),
            ("integer", json!(1), json!("x")),
            ("number", json!(1.5), json!("x")),
            ("boolean", json!(true), json!("x")),
            ("array", json!([]), json!("x")),
            ("object", json!({}), json!("x")),
        ];
        for (name, good, bad) in cases {
            let s = json!({"type": "object", "properties": {"v": {"type": name}}});
            assert!(
                validate(&s, &json!({"v": good})).is_empty(),
                "{name} rejected its own type"
            );
            assert!(
                !validate(&s, &json!({"v": bad})).is_empty(),
                "{name} accepted something else"
            );
        }
        // `null` is the one whose "wrong" value has to be something other than a string,
        // since a null property is indistinguishable from an absent one otherwise.
        let s = json!({"type": "object", "properties": {"v": {"type": "null"}}});
        assert!(validate(&s, &json!({"v": Value::Null})).is_empty());
        assert!(!validate(&s, &json!({"v": 1})).is_empty());
    }

    /// A negative integer is an integer. `as_u64` alone says otherwise, and a Unix timestamp
    /// before 1970 or a deliberately-past schedule is exactly where that would bite.
    #[test]
    fn a_negative_integer_is_still_an_integer() {
        let s = json!({"type": "object", "properties": {"when": {"type": "integer"}}});
        assert!(validate(&s, &json!({"when": -3600})).is_empty());
        assert!(!validate(&s, &json!({"when": -3600.5})).is_empty());
    }

    /// The two kinds of number are named differently, because "expected integer, got a
    /// number" would leave the caller guessing what was wrong with it.
    #[test]
    fn a_fractional_number_is_named_as_one() {
        let s = json!({"type": "object", "properties": {"n": {"type": "integer"}}});
        let fractional = validate(&s, &json!({"n": 1.5}));
        assert!(
            fractional[0].contains("a fractional number"),
            "{}",
            fractional[0]
        );

        let s = json!({"type": "object", "properties": {"n": {"type": "string"}}});
        let whole = validate(&s, &json!({"n": 2}));
        assert!(whole[0].contains("a number"), "{}", whole[0]);
        assert!(!whole[0].contains("fractional"), "{}", whole[0]);

        // An integer too large for an i64 is still an integer, not a float.
        let huge = validate(&s, &json!({"n": u64::MAX}));
        assert!(!huge[0].contains("fractional"), "{}", huge[0]);
    }

    /// The suggestion is only useful if the distance is right, and every arithmetic slip in
    /// it produces a plausible-looking number.
    #[test]
    fn distance_counts_single_edits() {
        assert_eq!(distance("status", "status"), 0);
        assert_eq!(
            distance("stauts", "status"),
            2,
            "a transposition is two edits"
        );
        assert_eq!(distance("statu", "status"), 1, "a deletion");
        assert_eq!(distance("statuss", "status"), 1, "an insertion");
        assert_eq!(distance("statut", "status"), 1, "a substitution");
        assert_eq!(
            distance("", "status"),
            6,
            "from nothing, one edit per character"
        );
        assert_eq!(distance("status", ""), 6);
        assert_eq!(distance("page", "per_page"), 4);
    }

    #[test]
    fn array_items_are_checked_individually() {
        let problems = validate(&schema(), &json!({"tags": ["rust", 7, "go"]}));
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("tags[1]"), "{}", problems[0]);
    }

    /// Every problem at once. A caller fixing one argument per round trip spends the rate
    /// budget learning what it could have been told in a single reply.
    #[test]
    fn every_problem_is_reported_together() {
        let problems = validate(
            &schema(),
            &json!({"status": "nope", "page": 0, "unknown": 1}),
        );
        assert_eq!(problems.len(), 3, "{problems:?}");
    }

    /// A missing argument is the tools' own business: they name the remedy, which this
    /// cannot. Enforcing `required` here would replace a good message with a generic one.
    #[test]
    fn a_missing_argument_is_left_to_the_tool_to_report() {
        let s = json!({
            "type": "object",
            "properties": {"article_id": {"type": "integer"}},
            "required": ["article_id"],
            "additionalProperties": false
        });
        assert!(validate(&s, &json!({})).is_empty());
        // What it does still catch is a required argument given the wrong way.
        assert!(!validate(&s, &json!({"article_id": "seven"})).is_empty());
    }
}
