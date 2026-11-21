//! The words of a search that its results highlight, as Discord's client
//! picks them (its `SearchHighlighter`): the searched text split at
//! anything but letters, digits and `_`, small letters, `'s` taken off,
//! common words left out, and each reduced to its stem by Porter's
//! stemmer, which the client uses. A word of a result is highlighted when
//! its stem is one of them.

use std::{collections::HashSet, ops::Range};

/// What the client leaves out of the words it highlights.
const STOPWORDS: [&str; 33] = [
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "if", "in", "into", "is", "it",
    "no", "not", "of", "on", "or", "such", "that", "the", "their", "then", "there", "these",
    "they", "this", "to", "was", "will", "with",
];

/// The stems a search highlights.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Words(HashSet<String>);

impl Words {
    /// The stems of the words `content` searched for.
    pub fn of(content: &str) -> Self {
        Self(
            content
                .split(|character: char| !is_word(character))
                .filter_map(stemmed)
                .collect(),
        )
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The byte ranges of `text` to highlight: each word whose stem is one
    /// of these.
    pub fn marks(&self, text: &str) -> Vec<Range<usize>> {
        if self.0.is_empty() {
            return Vec::new();
        }
        let mut marks = Vec::new();
        let mut start = None;
        let check = |range: Range<usize>, marks: &mut Vec<Range<usize>>| {
            if stemmed(&text[range.clone()]).is_some_and(|stem| self.0.contains(&stem)) {
                marks.push(range);
            }
        };
        for (at, character) in text.char_indices() {
            match (is_word(character), start) {
                (true, None) => start = Some(at),
                (false, Some(from)) => {
                    check(from..at, &mut marks);
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(from) = start {
            check(from..text.len(), &mut marks);
        }
        marks
    }
}

/// A word character as JavaScript's `\w` has it.
fn is_word(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

/// A word's stem, or none for a common word.
fn stemmed(word: &str) -> Option<String> {
    let word = word
        .strip_suffix("'s")
        .or_else(|| word.strip_suffix("'S"))
        .unwrap_or(word)
        .to_ascii_lowercase();
    (!word.is_empty() && !STOPWORDS.contains(&word.as_str())).then(|| stem(&word))
}

/// Porter's stemmer, as Martin Porter's own C version has it (with its
/// `bli` and `logi` rules), on a word in small ASCII letters.
pub fn stem(word: &str) -> String {
    if word.len() <= 2 || !word.is_ascii() {
        return word.to_owned();
    }
    let mut stemmer = Stemmer {
        b: word.as_bytes().to_vec(),
        k: word.len() - 1,
        j: 0,
    };
    stemmer.step1ab();
    if stemmer.k > 0 {
        stemmer.step1c();
        stemmer.step2();
        stemmer.step3();
        stemmer.step4();
        stemmer.step5();
    }
    stemmer.b.truncate(stemmer.k + 1);
    String::from_utf8(stemmer.b).unwrap_or_default()
}

/// The word being stemmed, `b[..=k]`, and `j`, where an ending just found
/// starts after.
struct Stemmer {
    b: Vec<u8>,
    k: usize,
    j: usize,
}

impl Stemmer {
    /// Whether `b[i]` is a consonant: a `y` is one after a vowel.
    fn cons(&self, i: usize) -> bool {
        match self.b[i] {
            b'a' | b'e' | b'i' | b'o' | b'u' => false,
            b'y' => i == 0 || !self.cons(i - 1),
            _ => true,
        }
    }

    /// How many vowel-consonant runs `b[..=j]` has.
    fn m(&self) -> usize {
        if self.j == usize::MAX {
            return 0;
        }
        let mut n = 0;
        let mut i = 0;
        loop {
            if i > self.j {
                return n;
            }
            if !self.cons(i) {
                break;
            }
            i += 1;
        }
        i += 1;
        loop {
            loop {
                if i > self.j {
                    return n;
                }
                if self.cons(i) {
                    break;
                }
                i += 1;
            }
            i += 1;
            n += 1;
            loop {
                if i > self.j {
                    return n;
                }
                if !self.cons(i) {
                    break;
                }
                i += 1;
            }
            i += 1;
        }
    }

    fn vowel_in_stem(&self) -> bool {
        self.j != usize::MAX && (0..=self.j).any(|i| !self.cons(i))
    }

    fn double_consonant(&self, j: usize) -> bool {
        j >= 1 && self.b[j] == self.b[j - 1] && self.cons(j)
    }

    /// Consonant, vowel, consonant at `i`, the last not w, x or y.
    fn cvc(&self, i: usize) -> bool {
        i >= 2
            && self.cons(i)
            && !self.cons(i - 1)
            && self.cons(i - 2)
            && !matches!(self.b[i], b'w' | b'x' | b'y')
    }

    /// Whether `b[..=k]` ends with `s`, which sets `j` before it.
    fn ends(&mut self, s: &str) -> bool {
        let s = s.as_bytes();
        if s.len() > self.k + 1 || &self.b[self.k + 1 - s.len()..=self.k] != s {
            return false;
        }
        // An ending as long as the word leaves nothing before it.
        self.j = (self.k + 1 - s.len()).wrapping_sub(1);
        true
    }

    fn set_to(&mut self, s: &str) {
        let start = self.j.wrapping_add(1);
        self.b.truncate(start);
        self.b.extend_from_slice(s.as_bytes());
        self.k = start + s.len() - 1;
    }

    fn r(&mut self, s: &str) {
        if self.j != usize::MAX && self.m() > 0 {
            self.set_to(s);
        }
    }

    fn step1ab(&mut self) {
        if self.b[self.k] == b's' {
            if self.ends("sses") {
                self.k -= 2;
            } else if self.ends("ies") {
                self.set_to("i");
            } else if self.b[self.k - 1] != b's' {
                self.k -= 1;
            }
        }
        if self.ends("eed") {
            if self.j != usize::MAX && self.m() > 0 {
                self.k -= 1;
            }
        } else if (self.ends("ed") || self.ends("ing"))
            && self.j != usize::MAX
            && self.vowel_in_stem()
        {
            self.k = self.j;
            if self.ends("at") {
                self.set_to("ate");
            } else if self.ends("bl") {
                self.set_to("ble");
            } else if self.ends("iz") {
                self.set_to("ize");
            } else if self.double_consonant(self.k) {
                self.k -= 1;
                if matches!(self.b[self.k], b'l' | b's' | b'z') {
                    self.k += 1;
                }
            } else {
                self.j = self.k;
                if self.m() == 1 && self.cvc(self.k) {
                    self.set_to("e");
                }
            }
        }
    }

    fn step1c(&mut self) {
        if self.ends("y") && self.j != usize::MAX && self.vowel_in_stem() {
            self.b[self.k] = b'i';
        }
    }

    /// The first of `rules` the word ends with, replaced where its stem
    /// is long enough; none after it are tried.
    fn first(&mut self, rules: &[(&str, &str)]) {
        for (ending, by) in rules {
            if self.ends(ending) {
                self.r(by);
                return;
            }
        }
    }

    fn step2(&mut self) {
        let rules: &[(&str, &str)] = match self.b[self.k - 1] {
            b'a' => &[("ational", "ate"), ("tional", "tion")],
            b'c' => &[("enci", "ence"), ("anci", "ance")],
            b'e' => &[("izer", "ize")],
            b'l' => &[
                ("bli", "ble"),
                ("alli", "al"),
                ("entli", "ent"),
                ("eli", "e"),
                ("ousli", "ous"),
            ],
            b'o' => &[("ization", "ize"), ("ation", "ate"), ("ator", "ate")],
            b's' => &[
                ("alism", "al"),
                ("iveness", "ive"),
                ("fulness", "ful"),
                ("ousness", "ous"),
            ],
            b't' => &[("aliti", "al"), ("iviti", "ive"), ("biliti", "ble")],
            b'g' => &[("logi", "log")],
            _ => &[],
        };
        self.first(rules);
    }

    fn step3(&mut self) {
        let rules: &[(&str, &str)] = match self.b[self.k] {
            b'e' => &[("icate", "ic"), ("ative", ""), ("alize", "al")],
            b'i' => &[("iciti", "ic")],
            b'l' => &[("ical", "ic"), ("ful", "")],
            b's' => &[("ness", "")],
            _ => &[],
        };
        self.first(rules);
    }

    fn step4(&mut self) {
        let endings: &[&str] = match self.b[self.k - 1] {
            b'a' => &["al"],
            b'c' => &["ance", "ence"],
            b'e' => &["er"],
            b'i' => &["ic"],
            b'l' => &["able", "ible"],
            b'n' => &["ant", "ement", "ment", "ent"],
            b'o' => {
                let ion = self.ends("ion")
                    && self.j != usize::MAX
                    && matches!(self.b[self.j], b's' | b't');
                if !ion && !self.ends("ou") {
                    return;
                }
                &[]
            }
            b's' => &["ism"],
            b't' => &["ate", "iti"],
            b'u' => &["ous"],
            b'v' => &["ive"],
            b'z' => &["ize"],
            _ => return,
        };
        if !endings.is_empty() && !endings.iter().any(|ending| self.ends(ending)) {
            return;
        }
        if self.j != usize::MAX && self.m() > 1 {
            self.k = self.j;
        }
    }

    fn step5(&mut self) {
        self.j = self.k;
        if self.b[self.k] == b'e' {
            let m = self.m();
            if m > 1 || (m == 1 && !self.cvc(self.k - 1)) {
                self.k -= 1;
            }
        }
        if self.b[self.k] == b'l' && self.double_consonant(self.k) && self.m() > 1 {
            self.k -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems_as_porters_examples() {
        for (word, stemmed) in [
            ("caresses", "caress"),
            ("ponies", "poni"),
            ("ties", "ti"),
            ("cats", "cat"),
            ("feed", "feed"),
            ("agreed", "agre"),
            ("plastered", "plaster"),
            ("motoring", "motor"),
            ("sing", "sing"),
            ("conflated", "conflat"),
            ("troubled", "troubl"),
            ("sized", "size"),
            ("hopping", "hop"),
            ("falling", "fall"),
            ("hissing", "hiss"),
            ("filing", "file"),
            ("happy", "happi"),
            ("sky", "sky"),
            ("relational", "relat"),
            ("conditional", "condit"),
            ("rational", "ration"),
            ("digitizer", "digit"),
            ("conformabli", "conform"),
            ("radicalli", "radic"),
            ("differentli", "differ"),
            ("vietnamization", "vietnam"),
            ("predication", "predic"),
            ("operator", "oper"),
            ("feudalism", "feudal"),
            ("decisiveness", "decis"),
            ("hopefulness", "hope"),
            ("callousness", "callous"),
            ("formaliti", "formal"),
            ("sensitiviti", "sensit"),
            ("sensibiliti", "sensibl"),
            ("triplicate", "triplic"),
            ("formative", "form"),
            ("formalize", "formal"),
            ("electrical", "electr"),
            ("hopeful", "hope"),
            ("goodness", "good"),
            ("revival", "reviv"),
            ("allowance", "allow"),
            ("inference", "infer"),
            ("airliner", "airlin"),
            ("adjustable", "adjust"),
            ("defensible", "defens"),
            ("irritant", "irrit"),
            ("replacement", "replac"),
            ("adjustment", "adjust"),
            ("dependent", "depend"),
            ("adoption", "adopt"),
            ("homologous", "homolog"),
            ("communism", "commun"),
            ("activate", "activ"),
            ("effective", "effect"),
            ("bowdlerize", "bowdler"),
            ("probate", "probat"),
            ("rate", "rate"),
            ("cease", "ceas"),
            ("controll", "control"),
            ("roll", "roll"),
            ("generalizations", "gener"),
            ("oscillators", "oscil"),
            ("analogousli", "analog"),
            ("archaeology", "archaeolog"),
            ("is", "is"),
            ("eed", "eed"),
            ("ing", "ing"),
        ] {
            assert_eq!(stem(word), stemmed, "{word}");
        }
    }

    #[test]
    fn marks_words_by_their_stems_but_not_common_ones() {
        let words = Words::of("the Trips planned");
        let text = "Planning a trip? The plan's in #trips_2026.";
        let marked: Vec<_> = words
            .marks(text)
            .into_iter()
            .map(|range| &text[range])
            .collect();
        // `plan's` is plan and s, as the client splits it; `trips_2026` is
        // one word, as `\w` has it, so it isn't trip.
        assert_eq!(marked, ["Planning", "trip", "plan"]);
        assert!(Words::of("the and of").is_empty());
        assert!(Words::of("").marks("anything").is_empty());
    }
}
