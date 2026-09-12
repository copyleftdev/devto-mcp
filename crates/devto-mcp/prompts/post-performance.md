Report on how {{scope}} is performing on dev.to.

**1. Get the numbers in one call.** Call `my_analytics`. It returns views, reactions,
comments, follower growth, referrers and top contributors together — use it rather than
several narrower calls, because the read budget is 30 requests per minute.

**2. Establish the baseline before saying anything is good or bad.** Call `my_articles` and
work out what this author's ordinary article does: median views, median reactions, the usual
ratio between them. A number is only high or low against that. "1,200 views" means nothing on
its own.

**3. Read the ratios, not just the totals.**

- *Views to reactions*: a high view count with few reactions usually means the title promised
  more than the article delivered.
- *Average read time against length*: readers leaving early is a structural problem, not a
  promotion problem.
- *Referrers*: search traffic and feed traffic behave differently. A post carried by search
  keeps earning; one carried by the feed stops within days.
- *Comments*: the one signal that someone finished it and had something to say.

**4. Say what you actually know.** dev.to reports no dates on individual referrers and no
per-article breakdown of follower growth, so do not attribute followers to a specific post.
Where the data does not support a conclusion, say that instead of reaching for one.

**5. Give one recommendation, not five.** Name the single change most likely to matter for the
next article, and the evidence for it.
