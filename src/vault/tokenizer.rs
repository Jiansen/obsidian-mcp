//! Script-aware tokenizer for mixed CJK / Latin full-text search (#114).
//!
//! Tantivy's built-in `default` / `en_stem` tokenizers split only on
//! whitespace and punctuation. Chinese (and other unspaced CJK) text has no
//! whitespace, so an entire sentence becomes ONE token and BM25 search only
//! matches byte-identical substrings — long queries like `凝结水处理系统`
//! returned 0 hits even when every word existed in the vault.
//!
//! Fix (industry-standard, cf. paperless-ngx bigram analyzer and
//! obsidian-second-brain #159): split text on script boundaries, then
//! - Latin / Cyrillic / Greek runs → whitespace/punctuation-separated words
//!   (behavior equal to tantivy `default`, incl. the 40-char token cap and
//!   lowercasing),
//! - CJK runs (Han + kana + Hangul) → overlapping character bigrams
//!   (single char if the run has length 1).
//!
//! Bigrams keep `凝结水处理系统` queryable as `凝结` `结水` `水处` `处理` `理系`
//! `系统` — the QueryParser tokenizes queries with the field's tokenizer, so
//! multi-token CJK terms automatically become phrase queries and no
//! dictionary is needed (water-treatment jargon like `混床` / `阴离子交换器`
//! just works, unlike dictionary segmenters).
//!
//! Stemming: the schema wires this tokenizer through tantivy's
//! `Stemmer(Language::English)` token filter (see `tantivy_index.rs`). The
//! Snowball English algorithm is an identity transform on pure-CJK tokens
//! (no ASCII vowel-consonant regions → no rules fire), so Latin keeps
//! `en_stem`-grade stemming while CJK bigrams pass through untouched.

use tantivy::tokenizer::{Token, TokenStream, Tokenizer};

/// CJK range check: Han, Hiragana, Katakana, Hangul syllables/jamo,
/// plus CJK symbol blocks that behave like unspaced script.
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x2E80..=0x2EFF   // CJK Radicals Supplement
        | 0x3000..=0x303F // CJK Symbols and Punctuation (、。「」…)
        | 0x3040..=0x309F // Hiragana
        | 0x30A0..=0x30FF // Katakana
        | 0x3130..=0x318F // Hangul Compatibility Jamo
        | 0x3400..=0x4DBF // CJK Ext A
        | 0x4E00..=0x9FFF // CJK Unified Ideographs
        | 0xAC00..=0xD7AF // Hangul Syllables
        | 0xF900..=0xFAFF // CJK Compatibility Ideographs
        | 0x20000..=0x2A6DF // CJK Ext B
    )
}

/// A maximal run of characters classified as CJK or non-CJK,
/// with its start offset in the source text.
struct ScriptRun<'a> {
    text: &'a str,
    start: usize,
    cjk: bool,
}

fn script_runs(text: &str) -> Vec<ScriptRun<'_>> {
    let mut runs = Vec::new();
    let mut start = 0usize;
    let mut prev_cjk: Option<bool> = None;
    for (idx, c) in text.char_indices() {
        let cjk = is_cjk(c);
        match prev_cjk {
            Some(prev) if prev != cjk => {
                runs.push(ScriptRun {
                    text: &text[start..idx],
                    start,
                    cjk: prev,
                });
                start = idx;
            }
            _ => {}
        }
        prev_cjk = Some(cjk);
    }
    if let Some(cjk) = prev_cjk {
        runs.push(ScriptRun {
            text: &text[start..],
            start,
            cjk,
        });
    }
    runs
}

/// Register name for the tokenizer (used by `Index::tokenizers()`).
pub const TOKENIZER_NAME: &str = "cjk_bigram";

/// Script-aware tokenizer producing the streams described in the module docs.
#[derive(Clone, Default)]
pub struct CjkBigramTokenizer;

pub struct CjkBigramTokenStream {
    tokens: Vec<Token>,
    index: usize,
}

impl Tokenizer for CjkBigramTokenizer {
    type TokenStream<'a> = CjkBigramTokenStream;

    fn token_stream<'a>(&mut self, text: &'a str) -> Self::TokenStream<'a> {
        let mut tokens: Vec<Token> = Vec::new();
        let mut pos = 0usize;

        for run in script_runs(text) {
            if run.cjk {
                let chars: Vec<(usize, char)> = run.text.char_indices().collect();
                if chars.len() == 1 {
                    // Run of length 1: emit a unigram (bigrams need 2 chars).
                    let (i, c) = chars[0];
                    tokens.push(Token {
                        offset_from: run.start,
                        offset_to: run.start + i + c.len_utf8(),
                        position: pos,
                        text: c.to_string(),
                        position_length: 1,
                    });
                    pos += 1;
                } else {
                    for w in chars.windows(2) {
                        let ((p, pc), (i, c)) = (w[0], w[1]);
                        let mut gram = String::with_capacity(pc.len_utf8() + c.len_utf8());
                        gram.push(pc);
                        gram.push(c);
                        tokens.push(Token {
                            offset_from: run.start + p,
                            offset_to: run.start + i + c.len_utf8(),
                            position: pos,
                            text: gram,
                            position_length: 1,
                        });
                        pos += 1;
                    }
                }
            } else {
                // Non-CJK: replicate tantivy `default` — split on
                // non-alphanumeric, lowercase, drop >40 char tokens.
                let mut word_start: Option<usize> = None;
                for (i, c) in run.text.char_indices() {
                    if c.is_alphanumeric() {
                        if word_start.is_none() {
                            word_start = Some(i);
                        }
                    } else if let Some(s) = word_start.take() {
                        push_word(run.text, run.start, s, i, &mut tokens, &mut pos);
                    }
                }
                if let Some(s) = word_start {
                    push_word(
                        run.text,
                        run.start,
                        s,
                        run.text.len(),
                        &mut tokens,
                        &mut pos,
                    );
                }
            }
        }

        CjkBigramTokenStream { tokens, index: 0 }
    }
}

fn push_word(
    run_text: &str,
    run_start: usize,
    from: usize,
    to: usize,
    tokens: &mut Vec<Token>,
    pos: &mut usize,
) {
    let raw = &run_text[from..to];
    let lowered = raw.to_lowercase();
    if lowered.chars().count() > 40 {
        return; // tantivy `default` drops >40 char tokens
    }
    tokens.push(Token {
        offset_from: run_start + from,
        offset_to: run_start + to,
        position: *pos,
        text: lowered,
        position_length: 1,
    });
    *pos += 1;
}

impl TokenStream for CjkBigramTokenStream {
    fn advance(&mut self) -> bool {
        if self.index < self.tokens.len() {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn token(&self) -> &Token {
        &self.tokens[self.index - 1]
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.tokens[self.index - 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tantivy::tokenizer::Tokenizer;

    fn collect(text: &str) -> Vec<String> {
        let mut t = CjkBigramTokenizer;
        let mut stream = t.token_stream(text);
        let mut out = Vec::new();
        while stream.advance() {
            out.push(stream.token().text.to_string());
        }
        out
    }

    fn collect_with_offsets(text: &str) -> Vec<(String, usize, usize)> {
        let mut t = CjkBigramTokenizer;
        let mut stream = t.token_stream(text);
        let mut out = Vec::new();
        while stream.advance() {
            let tok = stream.token();
            out.push((tok.text.to_string(), tok.offset_from, tok.offset_to));
        }
        out
    }

    #[test]
    fn cjk_run_becomes_overlapping_bigrams() {
        assert_eq!(
            collect("凝结水处理系统"),
            vec!["凝结", "结水", "水处", "处理", "理系", "系统"]
        );
    }

    #[test]
    #[allow(dead_code)]
    fn single_cjk_char_stays_unigram() {
        assert_eq!(collect("床"), vec!["床"]);
    }

    #[test]
    fn latin_words_lowercased_and_split() {
        assert_eq!(collect("Mixed Bed EDI"), vec!["mixed", "bed", "edi"]);
    }

    #[test]
    fn mixed_script_splits_cleanly() {
        assert_eq!(
            collect("凝结水Polishing系统"),
            vec!["凝结", "结水", "polishing", "系统"]
        );
    }

    #[test]
    fn cjk_punctuation_stays_inside_cjk_class() {
        assert_eq!(collect("阴床、阳床"), vec!["阴床", "床、", "、阳", "阳床"]);
    }

    #[test]
    fn mixed_number_and_cjk() {
        assert_eq!(
            collect("DN2000 阴离子交换器"),
            vec!["dn2000", "阴离", "离子", "子交", "交换", "换器"]
        );
    }

    #[test]
    fn empty_and_whitespace_only() {
        assert!(collect("").is_empty());
        assert!(collect("   \t\n ").is_empty());
    }

    #[test]
    fn long_latin_token_dropped_like_default_tokenizer() {
        let long = "a".repeat(41);
        assert!(collect(&long).is_empty());
        let ok = "a".repeat(40);
        assert_eq!(collect(&ok), vec![ok]);
    }

    #[test]
    fn offsets_are_byte_accurate() {
        // mixed script with ASCII prefix and CJK tail
        assert_eq!(
            collect_with_offsets("RO膜 system"),
            vec![
                ("ro".to_string(), 0, 2),
                ("膜".to_string(), 2, 5),
                ("system".to_string(), 6, 12),
            ]
        );
        assert_eq!(
            collect_with_offsets("凝结水"),
            vec![("凝结".to_string(), 0, 6), ("结水".to_string(), 3, 9),]
        );
    }
}

#[cfg(test)]
mod debug_tests {
    use super::*;
    use tantivy::tokenizer::Tokenizer;

    #[test]
    fn debug_multi_char_run() {
        let mut t = CjkBigramTokenizer;
        let mut stream = t.token_stream("混床工艺");
        let mut out = Vec::new();
        while stream.advance() {
            out.push(stream.token().text.to_string());
        }
        println!("混床工艺 -> {:?}", out);
        let mut t2 = CjkBigramTokenizer;
        let mut s2 = t2.token_stream("床");
        let mut out2 = Vec::new();
        while s2.advance() {
            out2.push(s2.token().text.to_string());
        }
        println!("床 -> {:?}", out2);
    }
}
