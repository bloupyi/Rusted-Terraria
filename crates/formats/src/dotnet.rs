//! Embedded resources of a .NET assembly (ECMA-335 `ManifestResource`), read
//! from the player's own `Terraria.exe`: the localization files live there.

use crate::reader::{bad, Error, Reader};

// Coded indexes: the tables each one can point to, by table number.
const TYPE_DEF_OR_REF: &[usize] = &[0x02, 0x01, 0x1B];
const HAS_CONSTANT: &[usize] = &[0x04, 0x08, 0x17];
const HAS_CUSTOM_ATTRIBUTE: &[usize] = &[
    0x06, 0x04, 0x01, 0x02, 0x08, 0x09, 0x0A, 0x00, 0x0E, 0x17, 0x14, 0x11, 0x1A, 0x1B, 0x20, 0x23, 0x26, 0x27, 0x28,
    0x2A, 0x2C, 0x2B,
];
const HAS_FIELD_MARSHAL: &[usize] = &[0x04, 0x08];
const HAS_DECL_SECURITY: &[usize] = &[0x02, 0x06, 0x20];
const MEMBER_REF_PARENT: &[usize] = &[0x02, 0x01, 0x1A, 0x06, 0x1B];
const HAS_SEMANTICS: &[usize] = &[0x14, 0x17];
const METHOD_DEF_OR_REF: &[usize] = &[0x06, 0x0A];
const MEMBER_FORWARDED: &[usize] = &[0x04, 0x06];
const IMPLEMENTATION: &[usize] = &[0x26, 0x23, 0x27];
// Five tags, of which only MethodDef (2) and MemberRef (3) are used.
const CUSTOM_ATTRIBUTE_TYPE: &[usize] = &[0x06, 0x0A, 0x06, 0x0A, 0x06];
const RESOLUTION_SCOPE: &[usize] = &[0x00, 0x1A, 0x23, 0x01];

#[derive(Clone, Copy)]
enum Col {
    U16,
    U32,
    Str,
    Guid,
    Blob,
    Table(usize),
    Coded(&'static [usize]),
}

use Col::*;

/// Columns of tables 0x00 to 0x27, the ones before `ManifestResource`.
fn schema(t: usize) -> &'static [Col] {
    match t {
        0x00 => &[U16, Str, Guid, Guid, Guid],
        0x01 => &[Coded(RESOLUTION_SCOPE), Str, Str],
        0x02 => &[U32, Str, Str, Coded(TYPE_DEF_OR_REF), Table(0x04), Table(0x06)],
        0x03 => &[Table(0x04)],
        0x04 => &[U16, Str, Blob],
        0x05 => &[Table(0x06)],
        0x06 => &[U32, U16, U16, Str, Blob, Table(0x08)],
        0x07 => &[Table(0x08)],
        0x08 => &[U16, U16, Str],
        0x09 => &[Table(0x02), Coded(TYPE_DEF_OR_REF)],
        0x0A => &[Coded(MEMBER_REF_PARENT), Str, Blob],
        0x0B => &[U16, Coded(HAS_CONSTANT), Blob],
        0x0C => &[Coded(HAS_CUSTOM_ATTRIBUTE), Coded(CUSTOM_ATTRIBUTE_TYPE), Blob],
        0x0D => &[Coded(HAS_FIELD_MARSHAL), Blob],
        0x0E => &[U16, Coded(HAS_DECL_SECURITY), Blob],
        0x0F => &[U16, U32, Table(0x02)],
        0x10 => &[U32, Table(0x04)],
        0x11 => &[Blob],
        0x12 => &[Table(0x02), Table(0x14)],
        0x13 => &[Table(0x14)],
        0x14 => &[U16, Str, Coded(TYPE_DEF_OR_REF)],
        0x15 => &[Table(0x02), Table(0x17)],
        0x16 => &[Table(0x17)],
        0x17 => &[U16, Str, Blob],
        0x18 => &[U16, Table(0x06), Coded(HAS_SEMANTICS)],
        0x19 => &[Table(0x02), Coded(METHOD_DEF_OR_REF), Coded(METHOD_DEF_OR_REF)],
        0x1A => &[Str],
        0x1B => &[Blob],
        0x1C => &[U16, Coded(MEMBER_FORWARDED), Str, Table(0x1A)],
        0x1D => &[U32, Table(0x04)],
        0x1E => &[U32, U32],
        0x1F => &[U32],
        0x20 => &[U32, U16, U16, U16, U16, U32, Blob, Str, Str],
        0x21 => &[U32],
        0x22 => &[U32, U32, U32],
        0x23 => &[U16, U16, U16, U16, U32, Blob, Str, Str, Blob],
        0x24 => &[U32, Table(0x23)],
        0x25 => &[U32, U32, U32, Table(0x23)],
        0x26 => &[U32, Str, Blob],
        0x27 => &[U32, U32, Str, Str, Coded(IMPLEMENTATION)],
        _ => &[],
    }
}

const MANIFEST_RESOURCE: usize = 0x28;

/// Every embedded resource of `exe` with its bytes.
pub fn resources(exe: &[u8]) -> Result<Vec<(String, &[u8])>, Error> {
    let mut r = Reader::new(exe);
    r.seek(0x3C)?;
    let pe = r.u32()? as usize;
    r.seek(pe)?;
    if r.bytes(4)? != b"PE\0\0" {
        return bad("not a PE file");
    }
    r.bytes(2)?;
    let sections = r.i16()? as usize;
    r.bytes(12)?;
    let optional = r.i16()? as usize;
    r.bytes(2)?;
    let opt = r.pos();
    let dirs = match r.i16()? as u16 {
        0x10B => opt + 96,
        0x20B => opt + 112,
        _ => return bad("unknown PE optional header"),
    };
    let mut table = Vec::new();
    r.seek(opt + optional)?;
    for _ in 0..sections {
        let h = r.bytes(40)?;
        let u = |o: usize| u32::from_le_bytes(h[o..o + 4].try_into().unwrap()) as usize;
        table.push((u(12), u(8).max(u(16)), u(20)));
    }
    let at = |rva: usize| -> Result<usize, Error> {
        table
            .iter()
            .find(|&&(va, size, _)| rva >= va && rva < va + size)
            .map(|&(va, _, raw)| rva - va + raw)
            .ok_or(Error::Format(format!("rva {rva:#x} outside the sections")))
    };
    r.seek(dirs + 14 * 8)?;
    let cli = at(r.u32()? as usize)?;
    r.seek(cli + 8)?;
    let meta = at(r.u32()? as usize)?;
    r.seek(cli + 24)?;
    let res = at(r.u32()? as usize)?;

    r.seek(meta)?;
    if r.u32()? != 0x424A_5342 {
        return bad("no CLI metadata");
    }
    r.bytes(8)?;
    let len = r.u32()? as usize;
    r.bytes(len + 2)?;
    let streams = r.i16()? as usize;
    let (mut tables, mut strings) = (None, None);
    for _ in 0..streams {
        let (off, size) = (r.u32()? as usize, r.u32()? as usize);
        let start = r.pos();
        let mut name = Vec::new();
        loop {
            match r.u8()? {
                0 => break,
                c => name.push(c),
            }
        }
        r.seek(start + (name.len() + 4) / 4 * 4)?;
        let s = exe.get(meta + off..meta + off + size).ok_or(Error::Eof)?;
        match name.as_slice() {
            b"#~" | b"#-" => tables = Some(s),
            b"#Strings" => strings = Some(s),
            _ => {}
        }
    }
    let (Some(tables), Some(strings)) = (tables, strings) else { return bad("metadata streams missing") };

    let mut t = Reader::new(tables);
    t.bytes(6)?;
    let heaps = t.u8()?;
    t.u8()?;
    let valid = t.u64()?;
    t.u64()?;
    let mut rows = [0usize; 64];
    for (i, n) in rows.iter_mut().enumerate() {
        if valid >> i & 1 != 0 {
            *n = t.u32()? as usize;
        }
    }
    let wide = |b: u8| if heaps & b != 0 { 4 } else { 2 };
    let size = |c: Col| match c {
        U16 => 2,
        U32 => 4,
        Str => wide(1),
        Guid => wide(2),
        Blob => wide(4),
        Table(i) => {
            if rows[i] < 1 << 16 {
                2
            } else {
                4
            }
        }
        Coded(tabs) => {
            let bits = usize::BITS - (tabs.len() - 1).leading_zeros();
            if tabs.iter().all(|&i| rows[i] < 1 << (16 - bits)) {
                2
            } else {
                4
            }
        }
    };
    for (i, &n) in rows.iter().enumerate().take(MANIFEST_RESOURCE) {
        if n > 0 && schema(i).is_empty() {
            return bad(format!("metadata table {i:#x} not handled"));
        }
        t.bytes(n * schema(i).iter().map(|&c| size(c)).sum::<usize>())?;
    }
    let read = |t: &mut Reader, n: usize| if n == 4 { t.u32() } else { t.i16().map(|v| v as u16 as u32) };
    let (str_size, impl_size) = (size(Str), size(Coded(IMPLEMENTATION)));
    let mut out = Vec::new();
    for _ in 0..rows[MANIFEST_RESOURCE] {
        let offset = t.u32()? as usize;
        t.u32()?;
        let name = read(&mut t, str_size)? as usize;
        let implementation = read(&mut t, impl_size)?;
        if implementation != 0 {
            continue;
        }
        let name = strings.get(name..).ok_or(Error::Eof)?;
        let name = String::from_utf8_lossy(&name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())]);
        r.seek(res + offset)?;
        let len = r.u32()? as usize;
        out.push((name.into_owned(), r.bytes(len)?));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn real_localization() {
        let Some(dir) = std::env::var_os(crate::install::ENV_VAR) else {
            eprintln!("TERRARIA_PATH not set, skipped");
            return;
        };
        let exe = std::fs::read(std::path::Path::new(&dir).join("Terraria.exe")).unwrap();
        let res = super::resources(&exe).unwrap();
        let (_, items) = res.iter().find(|(n, _)| n == "Terraria.Localization.Content.en-US.Items.json").unwrap();
        let text = std::str::from_utf8(items).unwrap();
        assert!(text.contains("\"ItemName\""));
    }
}
