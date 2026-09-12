Review dev.to draft {{article_id}} before it is published.

**1. Read it.** Call `my_articles` with `status: "unpublished"` and find the draft. That is
the only way to reach an unpublished article — there is no public URL for one.

**2. Run the mechanical checks.** Call `validate_draft` with the draft's fields. Report every
blocking finding and every warning. The warnings matter more here than the errors do: they
are the things dev.to will accept and then quietly do differently.

**3. Check the things validation cannot see.**

- *Tags*: do all of them already exist in the taxonomy? Call `list_tags` if unsure. A typo
  creates a new tag rather than failing.
- *Canonical URL*: if this was published elsewhere first, is it set? If it was not, is it
  absent? A canonical URL pointing at nothing costs the post its search ranking.
- *Cover image*: is the URL reachable and public? dev.to will not host it for you.
- *Series*: if it belongs to one, is the series named exactly as the other posts name it?
- *Disclosure*: does `ai_disclosure_level` match how the article was actually written?

**4. Read it as a reader would.** Does the title say what the article delivers? Does the first
paragraph earn the second? Are the code samples complete enough to run? Is there a reason for
someone to reach the end?

**5. Report, and stop.** Say what you would change and why. Do not publish — that is the
account holder's decision, and on dev.to the publication time is frozen the moment it is
made.
