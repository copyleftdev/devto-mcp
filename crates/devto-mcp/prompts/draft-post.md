Draft a dev.to article about: {{topic}}

Work in this order. It is arranged so that the expensive and irreversible things happen last.

**1. Find out what already exists.** Call `search_articles` with `mode: "semantic"` and a
description of the subject. Read what comes back before writing anything. If the ground is
well covered, the useful article is the one that says what the existing ones do not — say so
explicitly rather than writing the fifth introduction to the same topic.

**2. Choose tags against the real taxonomy.** Call `list_tags` and pick from what exists. A
tag that does not already exist gets created, which is almost never what anyone wants. Tags
are letters and digits only — no hyphens — so check that the tag you have in mind is spelled
the way dev.to spells it.

**3. Write it.** Markdown in `body_markdown`. Do not open the body with a front matter block:
front matter silently overrides the fields you pass, and mixing the two is how an article
ends up with tags nobody chose. For embeds use `{% embed <url> %}`, which resolves to the
right handler for the host.

**4. Check it before sending.** Call `validate_draft`. It costs nothing. Fix every blocking
finding and read the warnings — a warning means dev.to will accept the payload and then do
something other than what you asked.

**5. Create it as a draft.** Call `create_draft`. State `ai_disclosure_level` honestly: if you
wrote this text, it is `fully_autonomous` even though a human asked for it; `some_ai` is for
a human-authored piece you meaningfully assisted with. Do not claim `no_ai` for something you
produced.

Stop there and hand the draft back for review. Publishing is a separate decision and a
separate permission.
