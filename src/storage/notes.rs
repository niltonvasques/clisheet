use super::*;
use ironcalc::base::types::Comment;

pub(super) fn write_notes(
    parts: &mut Parts,
    sheet: &mut String,
    rels: &str,
    index: usize,
    comments: &[Comment],
) -> Result {
    if comments.is_empty() {
        return Ok(());
    }
    let mut authors = Vec::<&str>::new();
    let mut list = String::new();
    let mut shapes = String::new();
    for (i, note) in comments.iter().enumerate() {
        let (row, col) = parse_addr(&note.cell_ref)?;
        let author = note.author_name.as_str();
        let au = authors
            .iter()
            .position(|a| *a == author)
            .unwrap_or_else(|| {
                authors.push(author);
                authors.len() - 1
            });
        list.push_str(&format!(r#"<comment ref="{}" authorId="{au}"><text><t xml:space="preserve">{}</t></text></comment>"#,addr(row,col),esc(&note.text)));
        shapes.push_str(&format!(r##"<v:shape id="_x0000_s{}" type="#_x0000_t202" style="position:absolute;width:108pt;height:60pt;visibility:hidden"><v:textbox/><x:ClientData ObjectType="Note"><x:Row>{}</x:Row><x:Column>{}</x:Column></x:ClientData></v:shape>"##,1025+i,row-1,col-1));
    }
    let authors = authors
        .iter()
        .map(|name| format!("<author>{}</author>", esc(name)))
        .collect::<String>();
    parts.insert(format!("xl/comments{index}.xml"),format!(r#"<comments xmlns="{NS}"><authors>{authors}</authors><commentList>{list}</commentList></comments>"#).into_bytes());
    parts.insert(format!("xl/drawings/notes{index}.vml"),format!(r#"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:x="urn:schemas-microsoft-com:office:excel"><v:shapetype id="_x0000_t202" coordsize="21600,21600" path="m,l,21600r21600,l21600,xe"/>{shapes}</xml>"#).into_bytes());
    parts.entry(rels.into()).or_insert_with(||br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"></Relationships>"#.to_vec());
    append(
        parts,
        rels,
        "</Relationships>",
        &format!(
            r#"<Relationship Id="notes" Type="{REL}/comments" Target="../comments{index}.xml"/><Relationship Id="noteShapes" Type="{REL}/vmlDrawing" Target="../drawings/notes{index}.vml"/>"#
        ),
    )?;
    append(
        parts,
        "[Content_Types].xml",
        "</Types>",
        &format!(
            r#"<Override PartName="/xl/comments{index}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml"/><Override PartName="/xl/drawings/notes{index}.vml" ContentType="application/vnd.openxmlformats-officedocument.vmlDrawing"/>"#
        ),
    )?;
    let pos = ["<tableParts", "<extLst", "</worksheet>"]
        .iter()
        .filter_map(|tag| sheet.find(tag))
        .min()
        .ok_or("Estrutura XML inválida")?;
    sheet.insert_str(pos, r#"<legacyDrawing r:id="noteShapes"/>"#);
    Ok(())
}
