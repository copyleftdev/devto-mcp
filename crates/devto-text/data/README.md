# Third-party data

Both files are redistributed under permissive licences. Their terms are kept alongside them,
as both require.

## `cmudict_syllables.fst`

Derived from the [CMU Pronouncing Dictionary](https://github.com/cmusphinx/cmudict),
© 1993–2015 Carnegie Mellon University, **BSD-2-clause** — see `LICENSE-cmudict`.

Reduced to what a syllable count needs: for each word, the number of phonemes in its *first*
pronunciation whose last character is a digit, which is how CMUdict marks a stressed vowel.
Alternate pronunciations (`word(2)`) are dropped, because `textstat` reads index `[0]`.
123,455 entries, built by `scripts/build_syllable_fst.rs`.

**Extracted from NLTK's copy of CMUdict, not from `cmusphinx/cmudict` master.** NLTK ships an
older release, the two disagree on pronunciation order, and only the first pronunciation is
read — `extraordinary` is six syllables on master and five in NLTK's. `textstat` reads NLTK's,
so that is the edition parity requires.

## `hyph_en_US.dic`

American English hyphenation patterns from the Hunspell/LibreOffice project, converted by
László Németh from Knuth and Liang's plain TeX hyphenation table. **BSD-style**: unlimited
copying, redistribution and modification with the copyright and licence information — see
`README_hyph_en_US.txt`.

Used only for words CMUdict does not know, which is what `pyphen` does inside `textstat`.
