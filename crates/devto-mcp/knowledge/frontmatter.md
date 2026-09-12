# Front matter, and why it is not just another way to set the same fields

`body_markdown` may open with a Jekyll front matter block. Forem parses it — and then
**applies it over the top of the API payload**. `Article#evaluate_front_matter` runs in
`before_validation`, after the controller has assigned your parameters, and reassigns the
fields below from whatever the body says.

So this is not a second, equivalent input path. It is the winning one, and it wins silently:
there is no error, no warning in the response, and nothing in the API description that says
so.

```
tags: ["rust"]                    ← what you sent
---
tags: webdev, beginners           ← what the article ends up with
---
```

## What a front matter key displaces

| Front matter key | API field it overrides |
|---|---|
| `title` | `title` |
| `tags` | `tags` — **replaces**, never merges |
| `published` | `published` |
| `published_at`, `date` | `published_at` |
| `cover_image` | `main_image` |
| `canonical_url` | `canonical_url` |
| `description` | `description` |
| `series` | the article's series |

Note the cover image: the front matter spells it `cover_image`, the API field is
`main_image`. Two spellings for one concept, and mixing them loses the image without an
error.

## Three traps in that one method

**A title with no series removes the article from its series.** `evaluate_front_matter` does
`self.collection_id = nil if hash["title"].present?` and only restores it if the front matter
*also* names a series. Front matter carrying just a title quietly orphans the post.

**Tags replace rather than merge.** `set_tag_list` clears the list before adding, so front
matter tags are the complete set.

**The cover image becomes permanently front-matter-driven.** Once a cover image has been set
from front matter, `main_image_from_frontmatter` stays true forever. From then on, any body
with front matter but no `cover_image` key **clears the cover image**, and `main_image` in
the payload is ignored.

## Disclosure in front matter

`ai_disclosure_level` can be set here too, and Forem accepts a generous alias set:
`ai_disclosure`, or the booleans `ai_generated: true` and `ai_assisted: true`, or values like
`human`, `assisted`, `autonomous`, `100%_human`, or the bare enum integers 0, 1, 3 and 5.

## The recommendation

Pick one source of truth per field. Either send the fields in the payload and keep the body
free of front matter, or put everything in front matter and send only `body_markdown`. Mixing
them is how an article ends up with tags nobody chose.
