# Forem liquid tags

Liquid tags are what make a dev.to post a dev.to post, and none of them appear in the API
description — a client generated from the OpenAPI document has no idea they exist. They go
in `body_markdown` as `{% name argument %}`.

Derived from `app/liquid_tags/` in forem/forem at commit ac54b3b (2026-09-10): 74 registered names across 67 tags.

**When in doubt, use `{% embed <url> %}`.** Forem's unified embed resolves the URL to
whichever specific tag handles that host, and falls back to a link card when none does. It
is the right default unless you need a tag's own options.


## Runnable code and demos

Paste the URL of the pen, sandbox or repl. Most accept an optional height or file argument.

- `{% codepen … %}` — accepts codepen.io
- `{% codesandbox … %}` — accepts codesandbox.io
- `{% replit … %}` — accepts replit.com
- `{% stackblitz … %}` — accepts stackblitz.com, stackblitz.io
- `{% jsfiddle … %}` — accepts jsfiddle.net
- `{% jsitor … %}` — accepts jsitor.com
- `{% glitch … %}`
- `{% dotnetfiddle … %}` — accepts dotnetfiddle.net
- `{% livecodes … %}` — accepts livecodes.io, next.livecodes.io, v49.livecodes.io
- `{% runkit … %}` — block tag, needs `{% endrunkit %}`
- `{% nexttech … %}` — accepts nt.dev
- `{% gitpitch … %}` — accepts gitpitch.com
- `{% stackery … %}` — accepts app.stackery.io
- `{% cloudrun … %}` — accepts run.app
- `{% netlify … %}` — accepts netlify.app
- `{% neon … %}`
- `{% kotlin … %}`

## Code, repositories and terminals

Reference a file, repository, issue or recorded session.

- `{% gist … %}` — accepts gist.github.com
- `{% github … %}` — accepts github.com
- `{% asciinema … %}` — accepts asciinema.org
- `{% katex … %}` — block tag, needs `{% endkatex %}`
- `{% stackexchange … %}` — accepts api.stackexchange.com, stackexchange.com, stackoverflow.com
- `{% stackoverflow … %}` — accepts api.stackexchange.com, stackexchange.com, stackoverflow.com

## Video, audio and slides

The URL of the video, track or deck.

- `{% youtube … %}`
- `{% vimeo … %}` — accepts vimeo.com
- `{% twitch … %}` — accepts clips.twitch.tv, player.twitch.tv, twitch.tv
- `{% spotify … %}` — accepts open.spotify.com
- `{% soundcloud … %}` — accepts soundcloud.com
- `{% bandcamp … %}` — accepts bandcamp.com
- `{% blogcast … %}` — accepts blogcast.host
- `{% podcast … %}` — accepts d2uzvmey2c90kn.cloudfront.net, storage.googleapis.com, temenos.com
- `{% slideshare … %}` — accepts slideshare.net
- `{% speakerdeck … %}` — accepts speakerdeck.com
- `{% slides … %}` — block tag, needs `{% endslides %}`
- `{% slide … %}`

## Social posts

The URL or id of the post. These render a static card, not a live widget.

- `{% tweet … %}` — accepts platform.twitter.com, twitter.com
- `{% twitter … %}` — accepts platform.twitter.com, twitter.com
- `{% twitter_timeline … %}` — accepts platform.twitter.com, twitter.com
- `{% bluesky … %}` — accepts bsky.app, embed.bsky.app
- `{% instagram … %}` — accepts instagram.com
- `{% reddit … %}` — accepts reddit.com
- `{% medium … %}` — accepts medium.com
- `{% parler … %}` — accepts www.parler.io
- `{% wikipedia … %}` — accepts wikipedia.org

## Forem's own objects

Reference something that lives on dev.to itself — a person, a post, a discussion.

- `{% user … %}`
- `{% org … %}`
- `{% organization … %}`
- `{% comment … %}`
- `{% devcomment … %}`
- `{% post … %}`
- `{% link … %}`
- `{% tag … %}`
- `{% feed … %}`
- `{% org_posts … %}`
- `{% org_team … %}`

## Layout and presentation

Structure inside the article body. These are blocks: they need a matching end tag.

- `{% details … %}` — block tag, needs `{% enddetails %}`
- `{% collapsible … %}` — block tag, needs `{% endcollapsible %}`
- `{% spoiler … %}` — block tag, needs `{% endspoiler %}`
- `{% row … %}` — block tag, needs `{% endrow %}`
- `{% col … %}` — block tag, needs `{% endcol %}`
- `{% card … %}` — block tag, needs `{% endcard %}`
- `{% quote … %}` — block tag, needs `{% endquote %}`
- `{% quotes … %}` — block tag, needs `{% endquotes %}`
- `{% feature … %}` — block tag, needs `{% endfeature %}`
- `{% features … %}` — block tag, needs `{% endfeatures %}`

## Interaction and audience

Ask the reader something, or offer them something.

- `{% poll … %}`
- `{% survey … %}`
- `{% cta … %}` — block tag, needs `{% endcta %}`
- `{% offer … %}` — block tag, needs `{% endoffer %}`
- `{% event … %}`
- `{% user_subscription … %}`
- `{% org_lead_form … %}`
- `{% org_lead_gate … %}` — block tag, needs `{% endorg_lead_gate %}`

## Agent provenance

Attach an agent's working transcript to a post. Upload it through the agent sessions API first.

- `{% agent_session … %}`

## Two things that catch people out

A liquid tag that Forem cannot parse **fails the whole save** with a rendering error, not a
warning — a typo in a tag name loses the write. And `{% embed %}` needs the canonical URL of
the thing: a share link, a shortened link or a URL carrying tracking parameters will often
resolve to a plain link card instead of the embed you wanted.

