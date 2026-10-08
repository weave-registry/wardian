//! Text tools: counts, word frequencies, SHA-256 and case changes, done in WebAssembly.
//!
//! WebAssembly functions take only numbers, so text travels through the module's memory:
//!   1. JavaScript asks for space:      ptr = alloc(len)
//!   2. JavaScript copies the bytes in:  new Uint8Array(memory.buffer, ptr, len).set(bytes)
//!   3. JavaScript calls a function:     n = stats(ptr, len)
//!   4. JavaScript reads the answer:     n bytes at out_ptr(), as UTF-8 text
//!   5. JavaScript frees its space:      dealloc(ptr, len)
//!
//! Every function except `sha256` expects UTF-8 text. `sha256` hashes any bytes.
//! The answer stays in one buffer owned by the module until the next call replaces it.

use std::alloc::{alloc as raw_alloc, dealloc as raw_dealloc, Layout};
use std::collections::HashMap;
use std::fmt::Write;

// ---------------------------------------------------------------- memory

/// Reserves `len` bytes in the module's memory and returns where they start.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::NonNull::dangling().as_ptr();
    }
    unsafe { raw_alloc(Layout::from_size_align_unchecked(len, 1)) }
}

/// Gives back space from `alloc`.
#[no_mangle]
pub extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    if len != 0 {
        unsafe { raw_dealloc(ptr, Layout::from_size_align_unchecked(len, 1)) }
    }
}

static mut OUT: Vec<u8> = Vec::new();

/// Where the last answer starts. Its length is what the call returned.
#[no_mangle]
pub extern "C" fn out_ptr() -> *const u8 {
    unsafe { (*std::ptr::addr_of!(OUT)).as_ptr() }
}

/// Keeps `s` as the answer and returns its length in bytes.
fn answer(s: String) -> usize {
    let len = s.len();
    unsafe { *std::ptr::addr_of_mut!(OUT) = s.into_bytes() };
    len
}

fn bytes<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(ptr, len) }
}

fn text<'a>(ptr: *const u8, len: usize) -> std::borrow::Cow<'a, str> {
    String::from_utf8_lossy(bytes(ptr, len))
}

// ---------------------------------------------------------------- counts

const READ_WPM: f64 = 238.0; // average silent reading speed for adults
const SPEAK_WPM: f64 = 140.0; // a calm presentation pace

fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn is_apostrophe(c: char) -> bool {
    c == '\'' || c == '\u{2019}'
}

/// Words for frequency: runs of letters and digits, with apostrophes inside ("don't"), lower case.
fn word_tokens(s: &str) -> impl Iterator<Item = String> + '_ {
    s.split(|c: char| !(c.is_alphanumeric() || is_apostrophe(c)))
        .map(|w| w.trim_matches(is_apostrophe))
        .filter(|w| !w.is_empty())
        .map(|w| w.replace('\u{2019}', "'").to_lowercase())
}

struct Stats {
    chars: usize,
    chars_no_spaces: usize,
    words: usize,
    sentences: usize,
    lines: usize,
    paragraphs: usize,
    letters_in_words: usize,
    longest: String,
}

fn count(s: &str) -> Stats {
    let mut st = Stats {
        chars: 0,
        chars_no_spaces: 0,
        words: 0,
        sentences: 0,
        lines: 0,
        paragraphs: 0,
        letters_in_words: 0,
        longest: String::new(),
    };
    for c in s.chars() {
        st.chars += 1;
        if !c.is_whitespace() {
            st.chars_no_spaces += 1;
        }
    }
    // A word is a run of characters between spaces or dashes, with at least one letter or digit
    // in it: "well-known" is one word, "ago—never" is two, a lone "—" is none.
    for w in s.split(|c: char| c.is_whitespace() || c == '—' || c == '–') {
        if w.chars().any(char::is_alphanumeric) {
            st.words += 1;
            let core = w.trim_matches(|c: char| !c.is_alphanumeric());
            let n = core.chars().count();
            st.letters_in_words += n;
            if n > st.longest.chars().count() {
                st.longest = core.to_string();
            }
        }
    }
    // A sentence ends at . ! ? or … followed by a space or the end, once it has a word in it.
    let chars: Vec<char> = s.chars().collect();
    let mut open = false;
    for (i, &c) in chars.iter().enumerate() {
        if c.is_alphanumeric() {
            open = true;
        } else if open && matches!(c, '.' | '!' | '?' | '…' | '。' | '！' | '？') {
            let next = chars[i + 1..].iter().find(|c| !matches!(c, '.' | '!' | '?' | '…' | '"' | '\'' | '”' | '’' | ')' | ']'));
            if next.map_or(true, |c| c.is_whitespace()) {
                st.sentences += 1;
                open = false;
            }
        }
    }
    if open {
        st.sentences += 1;
    }
    // Lines: one more than the line breaks, not counting a final one. Paragraphs: runs of non-blank lines.
    if !s.is_empty() {
        let body = s.strip_suffix('\n').unwrap_or(s);
        st.lines = body.split('\n').count();
        let mut in_para = false;
        for line in body.split('\n') {
            let blank = line.trim().is_empty();
            if !blank && !in_para {
                st.paragraphs += 1;
            }
            in_para = !blank;
        }
    }
    st
}

/// Counts in the text, as JSON: chars, chars_no_spaces, bytes, words, unique_words, sentences,
/// lines, paragraphs, avg_word_length, longest_word, reading_seconds, speaking_seconds.
#[no_mangle]
pub extern "C" fn stats(ptr: *const u8, len: usize) -> usize {
    let s = text(ptr, len);
    let st = count(&s);
    let mut unique: HashMap<String, ()> = HashMap::new();
    for w in word_tokens(&s) {
        unique.insert(w, ());
    }
    let avg = if st.words == 0 { 0.0 } else { st.letters_in_words as f64 / st.words as f64 };
    let mut out = String::with_capacity(256);
    let _ = write!(
        out,
        "{{\"chars\":{},\"chars_no_spaces\":{},\"bytes\":{},\"words\":{},\"unique_words\":{},\"sentences\":{},\"lines\":{},\"paragraphs\":{},\"avg_word_length\":{:.1},\"reading_seconds\":{},\"speaking_seconds\":{},\"longest_word\":",
        st.chars,
        st.chars_no_spaces,
        len,
        st.words,
        unique.len(),
        st.sentences,
        st.lines,
        st.paragraphs,
        avg,
        (st.words as f64 * 60.0 / READ_WPM).round(),
        (st.words as f64 * 60.0 / SPEAK_WPM).round(),
    );
    json_str(&mut out, &st.longest);
    out.push('}');
    answer(out)
}

// ---------------------------------------------------------------- word frequency

/// Common English words that say little about what a text is about.
const COMMON: &[&str] = &[
    "a", "about", "after", "all", "also", "am", "an", "and", "any", "are", "as", "at", "be", "because",
    "been", "but", "by", "can", "could", "did", "do", "does", "for", "from", "had", "has", "have", "he",
    "her", "him", "his", "how", "i", "if", "in", "into", "is", "it", "it's", "its", "just", "me", "more",
    "my", "no", "not", "of", "on", "one", "only", "or", "our", "out", "she", "so", "some", "than", "that",
    "the", "their", "them", "then", "there", "these", "they", "this", "to", "up", "us", "was", "we",
    "were", "what", "when", "which", "who", "will", "with", "would", "you", "your",
];

/// The `limit` most frequent words, as JSON: {"total": words counted, "rows": [["word", count], …]}.
/// Ties sort alphabetically. With `skip_common` = 1, common English words are left out.
#[no_mangle]
pub extern "C" fn top_words(ptr: *const u8, len: usize, limit: u32, skip_common: u32) -> usize {
    let s = text(ptr, len);
    let mut counts: HashMap<String, u32> = HashMap::new();
    let mut total = 0u32;
    for w in word_tokens(&s) {
        if skip_common != 0 && COMMON.contains(&w.as_str()) {
            continue;
        }
        total += 1;
        *counts.entry(w).or_insert(0) += 1;
    }
    let mut rows: Vec<(String, u32)> = counts.into_iter().collect();
    rows.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    rows.truncate(limit as usize);
    let mut out = String::with_capacity(32 + rows.len() * 16);
    let _ = write!(out, "{{\"total\":{},\"rows\":[", total);
    for (i, (w, n)) in rows.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('[');
        json_str(&mut out, w);
        let _ = write!(out, ",{}]", n);
    }
    out.push_str("]}");
    answer(out)
}

// ---------------------------------------------------------------- SHA-256 (FIPS 180-4)

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn compress(h: &mut [u32; 8], block: &[u8]) {
    let mut w = [0u32; 64];
    for i in 0..16 {
        w[i] = u32::from_be_bytes([block[4 * i], block[4 * i + 1], block[4 * i + 2], block[4 * i + 3]]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
        *x = x.wrapping_add(y);
    }
}

fn sha256_digest(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let mut chunks = data.chunks_exact(64);
    for block in &mut chunks {
        compress(&mut h, block);
    }
    // Padding: a 1 bit, zeros, then the length in bits as a 64-bit big-endian number.
    let rest = chunks.remainder();
    let mut tail = [0u8; 128];
    tail[..rest.len()].copy_from_slice(rest);
    tail[rest.len()] = 0x80;
    let n = if rest.len() < 56 { 64 } else { 128 };
    tail[n - 8..n].copy_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in tail[..n].chunks_exact(64) {
        compress(&mut h, block);
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// SHA-256 of the bytes, as 64 lower-case hex digits.
#[no_mangle]
pub extern "C" fn sha256(ptr: *const u8, len: usize) -> usize {
    let mut hex = String::with_capacity(64);
    for b in sha256_digest(bytes(ptr, len)) {
        let _ = write!(hex, "{:02x}", b);
    }
    answer(hex)
}

// ---------------------------------------------------------------- case changes

/// Folds common accented Latin letters to plain ASCII, for slugs: é → e, ß → ss.
fn fold(c: char, out: &mut String) {
    let s = match c {
        'à'..='å' | 'ā' | 'ă' | 'ą' => "a",
        'æ' => "ae",
        'ç' | 'ć' | 'č' => "c",
        'ď' | 'đ' | 'ð' => "d",
        'è'..='ë' | 'ē' | 'ė' | 'ę' | 'ě' => "e",
        'ì'..='ï' | 'ī' | 'į' | 'ı' => "i",
        'ł' | 'ľ' => "l",
        'ñ' | 'ń' | 'ň' => "n",
        'ò'..='ö' | 'ø' | 'ō' | 'ő' => "o",
        'œ' => "oe",
        'ř' => "r",
        'ś' | 'š' | 'ş' => "s",
        'ß' => "ss",
        'ť' | 'ţ' => "t",
        'þ' => "th",
        'ù'..='ü' | 'ū' | 'ů' | 'ű' | 'ų' => "u",
        'ý' | 'ÿ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        _ => return,
    };
    out.push_str(s);
}

fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_word = false;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if in_word {
                out.extend(c.to_lowercase());
            } else {
                out.extend(c.to_uppercase());
            }
            in_word = true;
        } else {
            // An apostrophe between letters keeps the word going: "don't", not "Don'T".
            in_word = in_word && is_apostrophe(c);
            out.push(c);
        }
    }
    out
}

fn sentence_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut start = true; // the next letter starts a sentence
    let mut after_end = false; // just passed . ! or ?
    for c in s.chars() {
        if c.is_alphanumeric() {
            if start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start = false;
            after_end = false;
        } else {
            if matches!(c, '.' | '!' | '?' | '…') {
                after_end = true;
            } else if c.is_whitespace() && after_end {
                start = true;
            } else if c == '\n' {
                start = true;
            }
            out.push(c);
        }
    }
    out
}

fn slug(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if c.is_alphanumeric() {
            let before = out.len();
            fold(c, &mut out);
            if out.len() == before && !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

/// Changes the case of the text. Mode 0: UPPER, 1: lower, 2: Title Case, 3: Sentence case,
/// 4: a-url-slug. Returns the answer's length; read it at out_ptr().
#[no_mangle]
pub extern "C" fn change_case(ptr: *const u8, len: usize, mode: u32) -> usize {
    let s = text(ptr, len);
    answer(match mode {
        0 => s.to_uppercase(),
        1 => s.to_lowercase(),
        2 => title_case(&s),
        3 => sentence_case(&s),
        _ => slug(&s),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out() -> String {
        unsafe { String::from_utf8((*std::ptr::addr_of!(OUT)).clone()).unwrap() }
    }

    // Tests run on several threads, and the answer buffer is shared: take turns.
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn run(f: impl Fn(*const u8, usize) -> usize, s: &str) -> String {
        let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let n = f(s.as_ptr(), s.len());
        let o = out();
        assert_eq!(n, o.len());
        o
    }

    #[test]
    fn hashes() {
        assert_eq!(run(|p, l| sha256(p, l), ""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(run(|p, l| sha256(p, l), "abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(
            run(|p, l| sha256(p, l), "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(run(|p, l| sha256(p, l), &"a".repeat(1_000_000)), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
        // 55, 56 and 64 bytes: the padding edges.
        assert_eq!(run(|p, l| sha256(p, l), &"a".repeat(55)), "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318");
        assert_eq!(run(|p, l| sha256(p, l), &"a".repeat(56)), "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a");
        assert_eq!(run(|p, l| sha256(p, l), &"a".repeat(64)), "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb");
    }

    #[test]
    fn counts() {
        let s = count("Hello, world! It's a well-known fact — isn't it?\n\nNew paragraph here.\n");
        assert_eq!(s.words, 11);
        assert_eq!(s.sentences, 3);
        assert_eq!(s.lines, 3);
        assert_eq!(s.paragraphs, 2);
        assert_eq!(s.longest, "well-known");
        assert_eq!(count("ago—never mind").words, 3);
        assert_eq!(count("").lines, 0);
        assert_eq!(count("e.g. this").sentences, 2); // abbreviations count as an end: a known limit
        assert_eq!(count("Wait... what?! Yes.").sentences, 3);
        assert_eq!(count("He said \"stop.\" Then left").sentences, 2);
        let j = run(|p, l| stats(p, l), "Two words");
        assert!(j.contains("\"words\":2"), "{j}");
        assert!(j.contains("\"longest_word\":\"words\""), "{j}");
    }

    #[test]
    fn words() {
        let j = run(|p, l| top_words(p, l, 3, 0), "The cat and the hat. THE end, don’t.");
        assert_eq!(j, "{\"total\":8,\"rows\":[[\"the\",3],[\"and\",1],[\"cat\",1]]}");
        let j = run(|p, l| top_words(p, l, 5, 1), "The cat and the hat.");
        assert_eq!(j, "{\"total\":2,\"rows\":[[\"cat\",1],[\"hat\",1]]}");
        let j = run(|p, l| top_words(p, l, 5, 0), "say \"hi\"\n");
        assert_eq!(j, "{\"total\":2,\"rows\":[[\"hi\",1],[\"say\",1]]}");
    }

    #[test]
    fn cases() {
        let c = |s: &str, m| run(|p, l| change_case(p, l, m), s);
        assert_eq!(c("Straße", 0), "STRASSE");
        assert_eq!(c("hELLO wORLD, don't STOP", 2), "Hello World, Don't Stop");
        assert_eq!(c("hello. WORLD is big! ok\nnext", 3), "Hello. World is big! Ok\nNext");
        assert_eq!(c("  Crème Brûlée: 10 Tips & Tricks!  ", 4), "creme-brulee-10-tips-tricks");
        assert_eq!(c("日本 text", 4), "text");
    }
}
