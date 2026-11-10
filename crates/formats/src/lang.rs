//! The game's texts as `LanguageManager` loads them: the JSON files embedded in
//! `Terraria.exe` for English, then those of the chosen culture over them,
//! then `{$Category.Key}` references replaced by the text they name.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::reader::{bad, Error};

const PREFIX: &str = "Terraria.Localization.Content.";
const FALLBACK: &str = "en-US";

#[derive(Default, Debug)]
pub struct Texts(HashMap<String, String>);

impl Texts {
    /// Texts of `culture` (`fr-FR`...) from the assembly bytes.
    pub fn load(exe: &[u8], culture: &str) -> Result<Self, Error> {
        let res = crate::dotnet::resources(exe)?;
        let mut texts = Texts::default();
        for c in [FALLBACK, culture] {
            let mut files: Vec<_> =
                res.iter().filter(|(n, _)| n.starts_with(&format!("{PREFIX}{c}")) && n.ends_with(".json")).collect();
            files.sort_by(|a, b| a.0.cmp(&b.0));
            for (_, data) in files {
                let text = std::str::from_utf8(data).map_err(|_| Error::Format("language file not UTF-8".into()))?;
                for (category, entries) in parse(text.trim_start_matches('\u{feff}'))? {
                    for (k, v) in entries {
                        texts.0.insert(format!("{category}.{k}"), v);
                    }
                }
            }
        }
        texts.resolve_copies();
        Ok(texts)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    /// Every character the texts use: the font glyphs worth decoding.
    pub fn chars(&self) -> std::collections::HashSet<char> {
        self.0.values().flat_map(|v| v.chars()).collect()
    }

    /// `ProcessCopyCommandsInTexts`: up to 100 rounds of `{$A.B}` replacement.
    fn resolve_copies(&mut self) {
        let keys: Vec<String> = self.0.keys().filter(|k| self.0[*k].contains("{$")).cloned().collect();
        for key in keys {
            let mut value = self.0[&key].clone();
            for _ in 0..100 {
                let next = replace_refs(&value, |r| self.0.get(r).cloned());
                if next == value {
                    break;
                }
                value = next;
            }
            self.0.insert(key, value);
        }
    }
}

fn replace_refs(s: &str, look: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("{$") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        let name_len = after.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.')).unwrap_or(after.len());
        let name = &after[..name_len];
        let valid = after[name_len..].starts_with('}') && name.split('.').count() == 2;
        if valid {
            out.push_str(&look(name).unwrap_or_else(|| name.to_string()));
            rest = &after[name_len + 1..];
        } else {
            out.push_str("{$");
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

type Categories = Vec<(String, Vec<(String, String)>)>;

/// A JSON object of objects of strings, which is all the language files hold.
fn parse(text: &str) -> Result<Categories, Error> {
    let mut p = Parser { s: text.as_bytes(), i: 0 };
    let mut out = Vec::new();
    p.expect(b'{')?;
    while !p.end(b'}')? {
        let category = p.string()?;
        p.expect(b':')?;
        p.expect(b'{')?;
        let mut entries = Vec::new();
        while !p.end(b'}')? {
            let k = p.string()?;
            p.expect(b':')?;
            entries.push((k, p.string()?));
        }
        out.push((category, entries));
    }
    Ok(out)
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn skip(&mut self) {
        while self.s.get(self.i).is_some_and(|c| c.is_ascii_whitespace()) {
            self.i += 1;
        }
    }

    fn expect(&mut self, c: u8) -> Result<(), Error> {
        self.skip();
        if self.s.get(self.i) != Some(&c) {
            return bad(format!("expected '{}' at byte {}", c as char, self.i));
        }
        self.i += 1;
        Ok(())
    }

    /// True on the closing `close`; otherwise steps over a separating comma.
    fn end(&mut self, close: u8) -> Result<bool, Error> {
        self.skip();
        match self.s.get(self.i) {
            Some(&c) if c == close => {
                self.i += 1;
                Ok(true)
            }
            Some(b',') => {
                self.i += 1;
                self.skip();
                // Json.NET takes a trailing comma.
                if self.s.get(self.i) == Some(&close) {
                    self.i += 1;
                    return Ok(true);
                }
                Ok(false)
            }
            Some(_) => Ok(false),
            None => Err(Error::Eof),
        }
    }

    fn string(&mut self) -> Result<String, Error> {
        self.expect(b'"')?;
        let mut out = Vec::new();
        loop {
            let c = *self.s.get(self.i).ok_or(Error::Eof)?;
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = *self.s.get(self.i).ok_or(Error::Eof)?;
                    self.i += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'u' => {
                            let hex = self.s.get(self.i..self.i + 4).ok_or(Error::Eof)?;
                            self.i += 4;
                            let mut v = u32::from_str_radix(std::str::from_utf8(hex).unwrap_or("x"), 16)
                                .map_err(|_| Error::Format("bad \\u escape".into()))?;
                            // A surrogate pair.
                            if (0xD800..0xDC00).contains(&v) && self.s.get(self.i..self.i + 2) == Some(b"\\u") {
                                let lo = self.s.get(self.i + 2..self.i + 6).ok_or(Error::Eof)?;
                                let lo = u32::from_str_radix(std::str::from_utf8(lo).unwrap_or("x"), 16).unwrap_or(0);
                                self.i += 6;
                                v = 0x10000 + ((v - 0xD800) << 10) + (lo.wrapping_sub(0xDC00) & 0x3FF);
                            }
                            let ch = char::from_u32(v).unwrap_or('\u{FFFD}');
                            out.extend_from_slice(ch.encode_utf8(&mut [0; 4]).as_bytes());
                        }
                        e => out.push(e),
                    }
                }
                c => out.push(c),
            }
        }
        String::from_utf8(out).map_err(|_| Error::Format("string not UTF-8".into()))
    }
}

/// The language the player chose in the game (`Language` in its `config.json`),
/// unless `language = ...` in `rusted-terraria.cfg` says otherwise; English by default.
pub fn culture() -> String {
    crate::install::config_value("language")
        .or_else(|| {
            let text = std::fs::read_to_string(game_config()?).ok()?;
            let i = text.find("\"Language\"")?;
            let rest = &text[i + 10..];
            let start = rest.find('"')? + 1;
            let end = start + rest[start..].find('"')?;
            Some(rest[start..end].to_string())
        })
        .unwrap_or_else(|| FALLBACK.to_string())
}

/// `Main.SavePath`'s `config.json`.
fn game_config() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    let dirs = [
        std::env::var_os("XDG_DATA_HOME").map(|d| Path::new(&d).join("Terraria")),
        home.as_ref().map(|h| h.join(".local/share/Terraria")),
        home.as_ref().map(|h| h.join("Library/Application Support/Terraria")),
        home.as_ref().map(|h| h.join("Documents/My Games/Terraria")),
    ];
    dirs.into_iter().flatten().map(|d| d.join("config.json")).find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_copies() {
        assert!(parse("").is_err());
        let text = r#"{ "A": { "x": "Hi \"you\"\né", "y": "{$A.x}!", }, "B": {} }"#;
        let cats = parse(text).unwrap();
        assert_eq!(cats[0].1[0].1, "Hi \"you\"\né");
        let mut t = Texts::default();
        for (c, e) in cats {
            for (k, v) in e {
                t.0.insert(format!("{c}.{k}"), v);
            }
        }
        t.0.insert("A.z".into(), "{$A.missing} {$nope}".into());
        t.resolve_copies();
        assert_eq!(t.get("A.y"), Some("Hi \"you\"\né!"));
        assert_eq!(t.get("A.z"), Some("A.missing {$nope}"));
        assert!(t.chars().contains(&'é'));
    }

    #[test]
    fn real_texts() {
        let Some(dir) = std::env::var_os(crate::install::ENV_VAR) else {
            eprintln!("TERRARIA_PATH not set, skipped");
            return;
        };
        let exe = std::fs::read(Path::new(&dir).join("Terraria.exe")).unwrap();
        let en = Texts::load(&exe, "en-US").unwrap();
        assert_eq!(en.get("LegacyInterface.0"), Some("Life:"));
        assert_eq!(en.get("ItemName.CopperPickaxe"), Some("Copper Pickaxe"));
        let fr = Texts::load(&exe, "fr-FR").unwrap();
        assert_ne!(fr.get("ItemName.CopperPickaxe"), en.get("ItemName.CopperPickaxe"));
        assert!(fr.0.len() >= en.0.len(), "English stays under what is missing");
    }
}
