# Translations

Yapfast's interface is written in English, and each other language is one
gettext file, `lang/<code>.po`, built into the program. Appearance in the
settings chooses the language; until someone does, Yapfast follows the
system's, and shows English where it has no translation.

## How text is translated

[`crates/yapfast-lang`](../crates/yapfast-lang) holds the languages
(`LANGUAGES`), the chosen one, and its file as read. The English text is the
key: a text the file lacks shows in English, and with English chosen nothing
is looked up.

- **Slint** marks text with `@tr("Mark as read")`. Values go in placeholders,
  `@tr("Kick {}", name)`, and a count picks the form:
  `@tr("{n} member" | "{n} members" % count)`. Slint asks its translator for
  each, which `src/ui/language.rs` sets to `yapfast_lang` (the `tr` feature of
  `i-slint-core`), and evaluates every `@tr` again when the language changes.
- **Rust** marks text with `yapfast_lang::tr!("Mark as read")`, a
  `&'static str`; with values, `tr!("Kick {}", name)` is a `String`; and
  `trn!("{n} member", "{n} members", count)` picks the form. A table of
  constant text stays English, and `yapfast_lang::text(entry.name)` translates
  a value as it's read out.

The text must be a literal, whole sentences rather than joined pieces, so a
translation can order its words. `{}` takes the next value, `{0}` the value at
that place, `{n}` the count, and `{{` is a brace.

Rust's text is translated when it's made. Choosing a language shows the
window again, so what the adapter makes on each refresh follows at once; text
the application or a network made earlier, such as a loaded audit log, stays
as it was until it's made again.

The file is read the first time its language is chosen, into sorted lists
that borrow the file's text, and stays for the life of the process. Text
Discord and Matrix send, such as messages, names and a server's own command
names, is never translated, and Yapfast still asks Discord for English
(`X-Discord-Locale`).

## A language's file

`lang/ru.po` is kept by hand, one entry per English text, each string on one
line:

```
msgid "Kick {}"
msgstr "Выгнать {}"

msgid "{n} member"
msgid_plural "{n} members"
msgstr[0] "{n} участник"
msgstr[1] "{n} участника"
msgstr[2] "{n} участников"

msgctxt "ThreadState"
msgid "Open"
msgstr "Открыта"
```

An entry applies wherever its text is. One with `msgctxt` holds in that
context only, for a text that means two things: a plain `@tr` has its
component's name as its context, `@tr("ThreadState" => "Open")` names one,
and Rust asks with `yapfast_lang::text_in("Tray", "Disconnect")`.

`scripts/check-translations` lists the marked text each file lacks, and with
`--unused` the entries nothing marks. The tests of `yapfast-lang` check each
file: no text twice, each translation with its text's placeholders, each
plural in all the language's forms.

## Adding a language

Add `lang/<code>.po`, and its entry to `LANGUAGES` in
`crates/yapfast-lang/src/lib.rs`: the code, the language's own name, the file
and the rule that gives a count's plural form.
