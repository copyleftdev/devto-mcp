"""Regenerate the liquid tag catalogue from a Forem checkout.

Called by refresh-knowledge.sh. The grouping is curated — it is the part that makes the list
useful — but the names and accepted hosts are read from the source so they cannot drift.
"""

import os
import re

FOREM = os.environ["FOREM"]
COMMIT = os.environ["COMMIT"]
DATE = os.environ["DATE"]
OUT = os.environ["OUT"]

TLDS = {"com", "org", "net", "io", "co", "tv", "me", "app", "dev", "host", "cloud", "fm",
        "to", "gg", "run", "xyz", "so", "ai", "it", "ly", "be", "sh", "tech", "site",
        "link", "page", "social"}
DOMAIN = re.compile(r"([a-z0-9][a-z0-9\-]{1,40}(?:\\?\.[a-z0-9\-]{2,20})+)")
SKIP = {"liquid_tag_base.rb", "unified_embed.rb"}

GROUPS = [
    ("Runnable code and demos",
     "Paste the URL of the pen, sandbox or repl. Most accept an optional height or file argument.",
     ["codepen", "codesandbox", "replit", "stackblitz", "jsfiddle", "jsitor", "glitch",
      "dotnetfiddle", "livecodes", "runkit", "nexttech", "gitpitch", "stackery", "cloudrun",
      "netlify", "neon", "kotlin"]),
    ("Code, repositories and terminals",
     "Reference a file, repository, issue or recorded session.",
     ["gist", "github", "asciinema", "katex", "stackexchange", "stackoverflow"]),
    ("Video, audio and slides",
     "The URL of the video, track or deck.",
     ["youtube", "vimeo", "twitch", "spotify", "soundcloud", "bandcamp", "blogcast",
      "podcast", "slideshare", "speakerdeck", "slides", "slide"]),
    ("Social posts",
     "The URL or id of the post. These render a static card, not a live widget.",
     ["tweet", "twitter", "twitter_timeline", "bluesky", "instagram", "reddit", "medium",
      "parler", "wikipedia"]),
    ("Forem's own objects",
     "Reference something that lives on dev.to itself — a person, a post, a discussion.",
     ["user", "org", "organization", "comment", "devcomment", "post", "link", "tag", "feed",
      "forem", "org_posts", "org_team"]),
    ("Layout and presentation",
     "Structure inside the article body. These are blocks: they need a matching end tag.",
     ["details", "collapsible", "spoiler", "row", "col", "card", "quote", "quotes",
      "feature", "features"]),
    ("Interaction and audience",
     "Ask the reader something, or offer them something.",
     ["poll", "survey", "cta", "offer", "event", "user_subscription", "org_lead_form",
      "org_lead_gate"]),
    ("Agent provenance",
     "Attach an agent's working transcript to a post. Upload it through the agent sessions "
     "API first.",
     ["agent_session"]),
]


def scan():
    directory = os.path.join(FOREM, "app", "liquid_tags")
    found = []
    for filename in sorted(os.listdir(directory)):
        if not filename.endswith(".rb") or filename in SKIP:
            continue
        source = open(os.path.join(directory, filename), encoding="utf-8",
                      errors="replace").read()
        names = sorted(set(re.findall(
            r'Liquid::Template\.register_(?:tag|block)\(\s*"([^"]+)"', source)))
        if not names:
            continue
        hosts = set()
        for line in source.splitlines():
            if "http" not in line:
                continue
            for match in DOMAIN.finditer(line):
                host = match.group(1).replace("\\", "").lower().rstrip(".")
                if host.split(".")[-1] in TLDS and len(host) > 4:
                    hosts.add(host)
        found.append({
            "names": names,
            "hosts": sorted(hosts)[:5],
            "block": bool(re.search(r"register_block\(|< *Liquid::Block", source)),
        })
    return found


def render(found):
    hosts = {n: e["hosts"] for e in found for n in e["names"]}
    blocks = {n: e["block"] for e in found for n in e["names"]}
    known = set(hosts)

    lines = [
        "# Forem liquid tags\n",
        "Liquid tags are what make a dev.to post a dev.to post, and none of them appear in "
        "the API\ndescription — a client generated from the OpenAPI document has no idea they "
        "exist. They go\nin `body_markdown` as `{% name argument %}`.\n",
        f"Derived from `app/liquid_tags/` in forem/forem at commit {COMMIT} ({DATE}): "
        f"{len(known)} registered names across {len(found)} tags.\n",
        "**When in doubt, use `{% embed <url> %}`.** Forem's unified embed resolves the URL "
        "to\nwhichever specific tag handles that host, and falls back to a link card when "
        "none does. It\nis the right default unless you need a tag's own options.\n",
    ]

    placed = set()
    for title, blurb, names in GROUPS:
        lines.append(f"\n## {title}\n\n{blurb}\n")
        for name in names:
            if name not in known:
                continue
            placed.add(name)
            bits = []
            if hosts.get(name):
                bits.append("accepts " + ", ".join(hosts[name]))
            if blocks.get(name):
                bits.append("block tag, needs `{% end" + name + " %}`")
            suffix = f" — {'; '.join(bits)}" if bits else ""
            lines.append(f"- `{{% {name} … %}}`{suffix}")

    leftover = sorted(known - placed)
    if leftover:
        lines.append("\n## Not yet grouped\n")
        lines.append("New since this catalogue was last curated.\n")
        for name in leftover:
            lines.append(f"- `{{% {name} … %}}`")

    lines.append(
        "\n## Two things that catch people out\n\n"
        "A liquid tag that Forem cannot parse **fails the whole save** with a rendering "
        "error, not a\nwarning — a typo in a tag name loses the write. And `{% embed %}` "
        "needs the canonical URL of\nthe thing: a share link, a shortened link or a URL "
        "carrying tracking parameters will often\nresolve to a plain link card instead of the "
        "embed you wanted.\n")
    return "\n".join(lines) + "\n"


found = scan()
open(OUT, "w", encoding="utf-8").write(render(found))
print(f"{len({n for e in found for n in e['names']})} tag names across {len(found)} tags")
