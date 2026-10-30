//! Preserve OOXML `t="str"` cells, which calamine 0.26 may parse as numbers.
use crate::error::{DeskError, Result};
use calamine::{Data, Range};
use quick_xml::{
    Reader,
    events::{BytesStart, Event},
};
use std::{collections::HashMap, fs::File, io::Read, path::Path};

fn invalid(error: impl std::fmt::Display) -> DeskError {
    DeskError::InvalidInput(format!("读取 Excel 文本单元格失败：{error}"))
}
fn attrs(event: &BytesStart<'_>, reader: &Reader<&[u8]>) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    for attr in event.attributes() {
        let attr = attr.map_err(invalid)?;
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let value = attr
            .decode_and_unescape_value(reader.decoder())
            .map_err(invalid)?
            .into_owned();
        out.insert(key, value);
    }
    Ok(out)
}
fn local(name: &[u8]) -> &[u8] {
    name.rsplit(|b| *b == b':').next().unwrap_or(name)
}
fn read_entry(zip: &mut zip::ZipArchive<File>, name: &str) -> Result<String> {
    let mut file = zip.by_name(name).map_err(invalid)?;
    let mut data = String::new();
    file.read_to_string(&mut data)?;
    Ok(data)
}
fn position(s: &str) -> Result<(u32, u32)> {
    let mut col = 0u32;
    let mut row = 0u32;
    let mut digits = false;
    for b in s.bytes() {
        if b.is_ascii_uppercase() && !digits {
            col = col
                .checked_mul(26)
                .and_then(|x| x.checked_add((b - b'A' + 1) as u32))
                .ok_or_else(|| invalid("列号溢出"))?;
        } else if b.is_ascii_digit() {
            digits = true;
            row = row
                .checked_mul(10)
                .and_then(|x| x.checked_add((b - b'0') as u32))
                .ok_or_else(|| invalid("行号溢出"))?;
        } else {
            return Err(invalid("单元格坐标无效"));
        }
    }
    if col == 0 || col > 16_384 || row == 0 || row > 1_048_576 {
        return Err(invalid("单元格坐标超出范围"));
    }
    Ok((row - 1, col - 1))
}

/// Returns raw string cells for the requested worksheets; non-OOXML formats need no overlay.
type TextCells = HashMap<String, Vec<((u32, u32), String)>>;
pub fn read(path: &Path, sheets: &[String]) -> Result<TextCells> {
    if !path
        .extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("xlsx") || x.eq_ignore_ascii_case("xlsm"))
    {
        return Ok(HashMap::new());
    }
    let mut zip = zip::ZipArchive::new(File::open(path)?).map_err(invalid)?;
    let workbook = read_entry(&mut zip, "xl/workbook.xml")?;
    let rels = read_entry(&mut zip, "xl/_rels/workbook.xml.rels")?;
    let mut ids = HashMap::new();
    let mut reader = Reader::from_str(&workbook);
    loop {
        match reader.read_event().map_err(invalid)? {
            Event::Start(e) | Event::Empty(e) if local(e.name().as_ref()) == b"sheet" => {
                let a = attrs(&e, &reader)?;
                if let (Some(name), Some(id)) = (a.get("name"), a.get("r:id"))
                    && sheets.contains(name)
                {
                    ids.insert(id.clone(), name.clone());
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut targets = HashMap::new();
    let mut reader = Reader::from_str(&rels);
    loop {
        match reader.read_event().map_err(invalid)? {
            Event::Start(e) | Event::Empty(e) if local(e.name().as_ref()) == b"Relationship" => {
                let a = attrs(&e, &reader)?;
                if let (Some(id), Some(target)) = (a.get("Id"), a.get("Target"))
                    && let Some(name) = ids.get(id)
                {
                    let target = target.trim_start_matches('/');
                    let full = if target.starts_with("xl/") {
                        target.to_string()
                    } else {
                        format!("xl/{target}")
                    };
                    if full.split('/').any(|part| part == ".." || part == ".")
                        || full.contains('\\')
                    {
                        return Err(invalid("工作表路径无效"));
                    }
                    targets.insert(name.clone(), full);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut output = HashMap::new();
    for name in sheets {
        let target = targets
            .get(name)
            .ok_or_else(|| invalid(format!("找不到工作表 {name} 的文件")))?;
        let xml = read_entry(&mut zip, target)?;
        let mut reader = Reader::from_str(&xml);
        let mut cell: Option<(u32, u32)> = None;
        let mut inside_value = false;
        let mut text = String::new();
        let mut found = Vec::new();
        loop {
            match reader.read_event().map_err(invalid)? {
                Event::Start(e) if local(e.name().as_ref()) == b"c" => {
                    let a = attrs(&e, &reader)?;
                    cell = if a.get("t").is_some_and(|t| t == "str") {
                        Some(position(
                            a.get("r").ok_or_else(|| invalid("缺少单元格坐标"))?,
                        )?)
                    } else {
                        None
                    };
                    text.clear();
                }
                Event::Start(e) if cell.is_some() && local(e.name().as_ref()) == b"v" => {
                    inside_value = true
                }
                Event::Text(e) if inside_value => text.push_str(&e.decode().map_err(invalid)?),
                Event::GeneralRef(e) if inside_value => {
                    let entity = format!("&{};", e.decode().map_err(invalid)?);
                    text.push_str(&quick_xml::escape::unescape(&entity).map_err(invalid)?);
                }
                Event::CData(e) if inside_value => text.push_str(&e.decode().map_err(invalid)?),
                Event::End(e) if local(e.name().as_ref()) == b"v" => inside_value = false,
                Event::End(e) if local(e.name().as_ref()) == b"c" => {
                    if let Some(pos) = cell.take() {
                        found.push((pos, text.clone()));
                    }
                    inside_value = false;
                }
                Event::Eof => break,
                _ => {}
            }
        }
        output.insert(name.clone(), found);
    }
    Ok(output)
}

pub fn apply(range: &mut Range<Data>, cells: &[((u32, u32), String)]) {
    if let (Some(start), Some(end)) = (range.start(), range.end()) {
        for (position, value) in cells {
            if position.0 >= start.0
                && position.1 >= start.1
                && position.0 <= end.0
                && position.1 <= end.1
            {
                range.set_value(*position, Data::String(value.clone()));
            }
        }
    }
}
