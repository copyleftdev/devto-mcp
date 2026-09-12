//! The knowledge layer: what a schema cannot carry.
//!
//! Tool descriptions can hold a rule or two. They cannot hold 74 liquid tags, the way front
//! matter overrides the payload, or dev.to's obligations for automated clients — and none of
//! that appears in the OpenAPI description either, so a generated client has no idea any of
//! it exists.
//!
//! These documents are compiled in rather than fetched. A resource that needs the network to
//! be readable is a resource that fails when the caller most needs it, and the content only
//! changes when Forem does. `scripts/refresh-knowledge.sh` re-derives the liquid tag
//! catalogue from a Forem checkout so the list cannot drift silently.

use serde_json::{Value, json};

pub struct Resource {
    pub uri: &'static str,
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub text: &'static str,
}

pub const RESOURCES: [Resource; 4] = [
    Resource {
        uri: "devto://liquid-tags",
        name: "liquid-tags",
        title: "Forem liquid tags",
        description: "Every embed and layout tag dev.to understands, grouped by purpose, with \
                      the hosts each one accepts. None of these appear in the API description.",
        text: include_str!("../knowledge/liquid-tags.md"),
    },
    Resource {
        uri: "devto://frontmatter",
        name: "frontmatter",
        title: "Front matter overrides the payload",
        description: "Which front matter keys displace which API fields, and the three ways a \
                      body block silently changes an article you thought you controlled.",
        text: include_str!("../knowledge/frontmatter.md"),
    },
    Resource {
        uri: "devto://constraints",
        name: "constraints",
        title: "What dev.to rejects, and what it accepts while doing something else",
        description: "The validation rules enforced in Forem's models rather than documented, \
                      plus the silent behaviours that produce no error at all.",
        text: include_str!("../knowledge/constraints.md"),
    },
    Resource {
        uri: "devto://governance",
        name: "governance",
        title: "What dev.to asks of automated clients",
        description: "dev.to's own obligations for software acting on an account holder's \
                      behalf, the AI disclosure ladder, and how this server holds to them.",
        text: include_str!("../knowledge/governance.md"),
    },
];

pub fn resource_list() -> Vec<Value> {
    RESOURCES
        .iter()
        .map(|r| {
            json!({
                "uri": r.uri,
                "name": r.name,
                "title": r.title,
                "description": r.description,
                "mimeType": "text/markdown",
            })
        })
        .collect()
}

pub fn read_resource(uri: &str) -> Option<Value> {
    RESOURCES.iter().find(|r| r.uri == uri).map(|r| {
        json!({
            "contents": [{
                "uri": r.uri,
                "name": r.name,
                "title": r.title,
                "mimeType": "text/markdown",
                "text": r.text,
            }]
        })
    })
}

pub struct Prompt {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    /// `(name, description, required)`. The placeholder in the body is `{{name}}`.
    pub arguments: &'static [(&'static str, &'static str, bool)],
    pub template: &'static str,
}

pub const PROMPTS: [Prompt; 3] = [
    Prompt {
        name: "draft-post",
        title: "Draft a dev.to article",
        description: "Research what already exists, choose real tags, write it, validate it, \
                      and leave it as a draft for review.",
        arguments: &[(
            "topic",
            "What the article should be about. A sentence is better than a keyword.",
            true,
        )],
        template: include_str!("../prompts/draft-post.md"),
    },
    Prompt {
        name: "pre-publish-review",
        title: "Review a draft before publishing",
        description: "Run the mechanical checks, then the ones validation cannot see: tags \
                      that exist, canonical URL, cover image, series, disclosure.",
        arguments: &[(
            "article_id",
            "The numeric id of the draft. Find it with my_articles.",
            true,
        )],
        template: include_str!("../prompts/pre-publish-review.md"),
    },
    Prompt {
        name: "post-performance",
        title: "Read the numbers against this author's own baseline",
        description: "Pull the analytics bundle in one call and interpret it against what this \
                      author's ordinary article does, rather than in the abstract.",
        arguments: &[(
            "scope",
            "What to report on — a specific article, or the account as a whole.",
            false,
        )],
        template: include_str!("../prompts/post-performance.md"),
    },
];

pub fn prompt_list() -> Vec<Value> {
    PROMPTS
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "title": p.title,
                "description": p.description,
                "arguments": p.arguments.iter().map(|(name, description, required)| json!({
                    "name": name,
                    "description": description,
                    "required": required,
                })).collect::<Vec<_>>(),
            })
        })
        .collect()
}

/// Fill a prompt's placeholders. An argument the caller left out keeps a readable stand-in
/// rather than becoming an empty hole in the middle of a sentence.
pub fn get_prompt(name: &str, arguments: &Value) -> Option<Value> {
    let prompt = PROMPTS.iter().find(|p| p.name == name)?;
    let mut text = prompt.template.to_string();

    for (arg_name, _, _) in prompt.arguments {
        let supplied = arguments
            .get(arg_name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty());
        let replacement = match supplied {
            Some(value) => value.to_string(),
            None => format!("(no {arg_name} given — ask for one)"),
        };
        text = text.replace(&format!("{{{{{arg_name}}}}}"), &replacement);
    }

    Some(json!({
        "description": prompt.description,
        "messages": [{
            "role": "user",
            "content": { "type": "text", "text": text }
        }]
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_resource_is_described_and_has_content() {
        assert_eq!(RESOURCES.len(), 4);
        let mut seen = std::collections::BTreeSet::new();
        for resource in &RESOURCES {
            assert!(resource.uri.starts_with("devto://"), "{}", resource.uri);
            assert!(seen.insert(resource.uri), "duplicate uri {}", resource.uri);
            assert!(
                resource.description.len() > 40,
                "{} needs a real description",
                resource.name
            );
            assert!(
                resource.text.len() > 500,
                "{} has almost no content",
                resource.name
            );
            assert!(
                resource.text.starts_with('#'),
                "{} is not markdown",
                resource.name
            );
        }
    }

    /// The documents exist to carry what the tool schemas cannot. If the specifics are not in
    /// them, the resource is decoration.
    #[test]
    fn the_documents_carry_the_specifics_they_promise() {
        let tags = RESOURCES[0].text;
        assert!(
            tags.contains("{% embed"),
            "the fallback tag should be named"
        );
        assert!(tags.contains("codepen") && tags.contains("agent_session"));
        assert!(tags.contains("74 registered names"));

        let frontmatter = RESOURCES[1].text;
        assert!(frontmatter.contains("cover_image"));
        assert!(frontmatter.contains("main_image"));
        assert!(
            frontmatter.contains("collection_id"),
            "the series trap should be explicit"
        );

        let constraints = RESOURCES[2].text;
        assert!(constraints.contains("800 KB"));
        assert!(constraints.contains("machinelearning"));
        assert!(
            constraints.contains("español"),
            "diacritics are allowed and worth saying"
        );
        assert!(constraints.contains("15 minutes"));

        let governance = RESOURCES[3].text;
        assert!(governance.contains("llms.txt"));
        assert!(governance.contains("fully_autonomous"));
        assert!(governance.contains("DEVTO_ALLOW_NO_AI_CLAIM"));
        assert!(governance.contains("DEVTO_PUBLISH"));
    }

    #[test]
    fn a_resource_is_read_by_uri_and_unknown_uris_are_not_invented() {
        let read = read_resource("devto://constraints").expect("known uri");
        assert_eq!(read["contents"][0]["uri"], json!("devto://constraints"));
        assert_eq!(read["contents"][0]["mimeType"], json!("text/markdown"));
        assert!(
            read["contents"][0]["text"]
                .as_str()
                .unwrap()
                .contains("800 KB")
        );

        assert!(read_resource("devto://nothing").is_none());
        assert!(read_resource("").is_none());
        assert!(read_resource("file:///etc/passwd").is_none());
    }

    #[test]
    fn the_resource_listing_matches_the_resources() {
        let listed = resource_list();
        assert_eq!(listed.len(), RESOURCES.len());
        for (entry, resource) in listed.iter().zip(RESOURCES.iter()) {
            assert_eq!(entry["uri"], json!(resource.uri));
            assert_eq!(entry["name"], json!(resource.name));
            assert_eq!(entry["mimeType"], json!("text/markdown"));
        }
    }

    #[test]
    fn every_prompt_declares_the_arguments_its_template_uses() {
        assert_eq!(PROMPTS.len(), 3);
        for prompt in &PROMPTS {
            assert!(!prompt.name.is_empty());
            assert!(prompt.description.len() > 40, "{}", prompt.name);
            assert!(!prompt.arguments.is_empty(), "{}", prompt.name);
            for (arg, description, _) in prompt.arguments {
                assert!(
                    prompt.template.contains(&format!("{{{{{arg}}}}}")),
                    "{} declares {arg} but never uses it",
                    prompt.name
                );
                assert!(description.len() > 20, "{}/{arg}", prompt.name);
            }
        }
    }

    #[test]
    fn a_prompt_substitutes_what_it_was_given() {
        let filled = get_prompt("draft-post", &json!({"topic": "zero-copy parsing in Rust"}))
            .expect("known prompt");
        let text = filled["messages"][0]["content"]["text"].as_str().unwrap();
        assert!(text.contains("zero-copy parsing in Rust"));
        assert!(!text.contains("{{topic}}"), "a placeholder survived");
        assert_eq!(filled["messages"][0]["role"], json!("user"));
    }

    /// A missing argument must leave something a reader can act on, not a blank.
    #[test]
    fn a_missing_argument_leaves_a_readable_stand_in() {
        for empty in [json!({}), json!({"topic": ""}), json!({"topic": "   "})] {
            let filled = get_prompt("draft-post", &empty).expect("known prompt");
            let text = filled["messages"][0]["content"]["text"].as_str().unwrap();
            assert!(!text.contains("{{topic}}"), "{empty}");
            assert!(text.contains("no topic given"), "{empty}");
        }
    }

    #[test]
    fn an_unknown_prompt_is_not_invented() {
        assert!(get_prompt("write-my-article", &json!({})).is_none());
        assert!(get_prompt("", &json!({})).is_none());
    }

    /// The prompts exist to encode the order of work, not to be polite.
    #[test]
    fn the_prompts_put_the_irreversible_step_last() {
        let draft = PROMPTS[0].template;
        assert!(draft.find("validate_draft").unwrap() < draft.find("create_draft").unwrap());
        assert!(draft.contains("semantic"), "research before writing");
        assert!(draft.contains("Publishing is a separate decision"));

        let review = PROMPTS[1].template;
        assert!(review.contains("Do not publish"));

        let performance = PROMPTS[2].template;
        assert!(
            performance.contains("baseline"),
            "a number means nothing without one"
        );
    }
}
