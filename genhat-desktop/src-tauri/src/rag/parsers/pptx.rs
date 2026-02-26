//! PPTX parser — text, image, and table extraction.
//!
//! Uses `zip` to unpack the PPTX and `xml-rs` for proper XML parsing of slides.
//! Extracts `<a:t>` text, `<p:pic>` images (resolved via relationships),
//! and `<a:tbl>` tables (rendered as PNG grid images).

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use xml::reader::{EventReader, XmlEvent};

use super::{
    ParsedDocument, ParsedElement, TextBlock,
    MIN_IMAGE_BYTES, MIN_IMAGE_HEIGHT, MIN_IMAGE_WIDTH,
};

pub fn parse(path: &Path, media_dir: Option<&Path>) -> Result<ParsedDocument, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Failed to open PPTX: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("ZIP error: {e}"))?;

    let title = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("presentation.pptx")
        .to_string();

    let mut sections = Vec::new();
    let mut elements = Vec::new();

    // Collect slide filenames and sort numerically
    let mut slide_names: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            let entry = archive.by_index(i).ok()?;
            let name = entry.name().to_string();
            if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
                Some(name)
            } else {
                None
            }
        })
        .collect();

    slide_names.sort_by(|a, b| {
        let num_a = extract_slide_number(a);
        let num_b = extract_slide_number(b);
        num_a.cmp(&num_b)
    });

    // Collect all media files from ppt/media/ for image extraction
    let media_files: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            let entry = archive.by_index(i).ok()?;
            let name = entry.name().to_string();
            if name.starts_with("ppt/media/") && !name.ends_with('/') {
                Some(name)
            } else {
                None
            }
        })
        .collect();

    for (idx, slide_name) in slide_names.iter().enumerate() {
        let slide_num = idx + 1;

        // Parse the slide relationships file for image references
        let rels_path = slide_name
            .replace("ppt/slides/", "ppt/slides/_rels/")
            + ".rels";
        let rels_map = parse_rels_file(&mut archive, &rels_path);

        // Read slide XML into a String first, then drop the ZipFile borrow
        let xml = {
            let mut xml_buf = String::new();
            if let Ok(mut entry) = archive.by_name(slide_name) {
                let _ = entry.read_to_string(&mut xml_buf);
            }
            xml_buf
        };

        if xml.is_empty() {
            continue;
        }

        let slide_content = parse_slide_xml(&xml);

        // Add text
        if !slide_content.text.trim().is_empty() {
            sections.push(TextBlock {
                text: slide_content.text.trim().to_string(),
                metadata: format!("slide:{slide_num}"),
            });
            elements.push(ParsedElement::text(
                slide_content.text.trim(),
                format!("slide:{slide_num}"),
            ));
        }

        let slide_context = slide_content.text.chars().take(400).collect::<String>();

        // Extract images referenced in this slide
        if let Some(media_dir) = media_dir {
            for (img_idx, embed_id) in slide_content.image_embed_ids.iter().enumerate() {
                if let Some(target) = rels_map.get(embed_id.as_str()) {
                    // Resolve relative path to full ZIP path
                    let media_path = if target.starts_with("../") {
                        format!("ppt/{}", target.trim_start_matches("../"))
                    } else {
                        format!("ppt/slides/{target}")
                    };

                    if let Ok(elem) = extract_pptx_image(
                        &mut archive,
                        &media_path,
                        media_dir,
                        slide_num,
                        img_idx + 1,
                        &slide_context,
                    ) {
                        if let Some(elem) = elem {
                            elements.push(elem);
                        }
                    }
                }
            }

            // Render tables to PNG
            for (tbl_idx, table) in slide_content.tables.iter().enumerate() {
                if !table.is_empty() {
                    // Build text representation for chunking
                    let tbl_text: String = table
                        .iter()
                        .map(|row| row.join("\t"))
                        .collect::<Vec<_>>()
                        .join("\n");

                    sections.push(TextBlock {
                        text: tbl_text.clone(),
                        metadata: format!("slide:{slide_num}:table:{}", tbl_idx + 1),
                    });

                    match render_pptx_table(
                        table,
                        media_dir,
                        slide_num,
                        (tbl_idx + 1) as u32,
                        &slide_context,
                    ) {
                        Ok(Some(elem)) => elements.push(elem),
                        _ => {
                            elements.push(ParsedElement::text(
                                &tbl_text,
                                format!("slide:{slide_num}:table:{}", tbl_idx + 1),
                            ));
                        }
                    }
                }
            }
        }
    }

    // Fallback: extract any remaining images from ppt/media/ not already handled
    if let Some(media_dir) = media_dir {
        let file2 = std::fs::File::open(path).map_err(|e| format!("Reopen PPTX: {e}"))?;
        let mut archive2 = zip::ZipArchive::new(file2).map_err(|e| format!("ZIP error: {e}"))?;

        for (i, media_name) in media_files.iter().enumerate() {
            if let Ok(elem) = extract_pptx_image(
                &mut archive2,
                media_name,
                media_dir,
                0, // unknown slide
                i + 1,
                "(presentation image)",
            ) {
                if let Some(elem) = elem {
                    // Deduplicate
                    if !elements.iter().any(|e| e.media_path == elem.media_path) {
                        elements.push(elem);
                    }
                }
            }
        }
    }

    Ok(ParsedDocument {
        title,
        elements,
        sections,
    })
}

/// Parsed content from a single slide.
struct SlideContent {
    text: String,
    image_embed_ids: Vec<String>,
    tables: Vec<Vec<Vec<String>>>, // table → row → cell
}

/// Parse slide XML using xml-rs, extracting text, image references, and tables.
fn parse_slide_xml(xml: &str) -> SlideContent {
    let reader = EventReader::from_str(xml);

    let mut text = String::new();
    let mut image_embed_ids = Vec::new();
    let mut tables: Vec<Vec<Vec<String>>> = Vec::new();

    let mut in_text_element = false;
    let mut in_table = false;
    let mut current_table: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut current_cell = String::new();
    let mut in_table_cell = false;

    for event in reader {
        match event {
            Ok(XmlEvent::StartElement {
                name, attributes, ..
            }) => {
                let local = name.local_name.as_str();
                match local {
                    // Text element
                    "t" if name.namespace_ref().map_or(false, |ns| ns.contains("drawingml")) => {
                        in_text_element = true;
                    }
                    // Image reference (blip)
                    "blip" => {
                        for attr in &attributes {
                            if attr.name.local_name == "embed" {
                                image_embed_ids.push(attr.value.clone());
                            }
                        }
                    }
                    // Table elements
                    "tbl" if name.namespace_ref().map_or(false, |ns| ns.contains("drawingml")) => {
                        in_table = true;
                        current_table = Vec::new();
                    }
                    "tr" if in_table => {
                        current_row = Vec::new();
                    }
                    "tc" if in_table => {
                        in_table_cell = true;
                        current_cell = String::new();
                    }
                    _ => {}
                }
            }
            Ok(XmlEvent::EndElement { name, .. }) => {
                let local = name.local_name.as_str();
                match local {
                    "t" => {
                        in_text_element = false;
                    }
                    "tbl" => {
                        if in_table && !current_table.is_empty() {
                            tables.push(current_table.clone());
                        }
                        in_table = false;
                    }
                    "tr" if in_table => {
                        if !current_row.is_empty() {
                            current_table.push(current_row.clone());
                        }
                    }
                    "tc" if in_table => {
                        current_row.push(current_cell.trim().to_string());
                        in_table_cell = false;
                    }
                    _ => {}
                }
            }
            Ok(XmlEvent::Characters(content)) | Ok(XmlEvent::CData(content)) => {
                if in_text_element {
                    if in_table_cell {
                        current_cell.push_str(&content);
                    } else {
                        text.push_str(&content);
                        text.push(' ');
                    }
                }
            }
            Err(e) => {
                log::debug!("XML parse warning: {e}");
                break;
            }
            _ => {}
        }
    }

    SlideContent {
        text,
        image_embed_ids,
        tables,
    }
}

/// Parse a .rels file from inside the PPTX ZIP.
/// Returns a map of relationship ID → target path.
fn parse_rels_file(
    archive: &mut zip::ZipArchive<std::fs::File>,
    rels_path: &str,
) -> HashMap<String, String> {
    let mut map = HashMap::new();

    if let Ok(mut entry) = archive.by_name(rels_path) {
        let mut xml = String::new();
        if entry.read_to_string(&mut xml).is_ok() {
            let reader = EventReader::from_str(&xml);
            for event in reader {
                if let Ok(XmlEvent::StartElement {
                    name, attributes, ..
                }) = event
                {
                    if name.local_name == "Relationship" {
                        let mut id = String::new();
                        let mut target = String::new();
                        for attr in &attributes {
                            match attr.name.local_name.as_str() {
                                "Id" => id = attr.value.clone(),
                                "Target" => target = attr.value.clone(),
                                _ => {}
                            }
                        }
                        if !id.is_empty() && !target.is_empty() {
                            map.insert(id, target);
                        }
                    }
                }
            }
        }
    }

    map
}

/// Extract an image from the PPTX ZIP and save as PNG.
fn extract_pptx_image(
    archive: &mut zip::ZipArchive<std::fs::File>,
    media_path: &str,
    media_dir: &Path,
    slide_num: usize,
    img_idx: usize,
    context: &str,
) -> Result<Option<ParsedElement>, String> {
    let mut entry = archive
        .by_name(media_path)
        .map_err(|e| format!("ZIP entry {media_path}: {e}"))?;

    let mut buf = Vec::new();
    entry
        .read_to_end(&mut buf)
        .map_err(|e| format!("Read {media_path}: {e}"))?;

    if buf.is_empty() {
        return Ok(None);
    }

    let dyn_img = image::load_from_memory(&buf)
        .map_err(|e| format!("Decode image {media_path}: {e}"))?;

    let (w, h) = (dyn_img.width(), dyn_img.height());
    if w < MIN_IMAGE_WIDTH || h < MIN_IMAGE_HEIGHT {
        return Ok(None);
    }

    let filename = format!("pptx_s{slide_num}_img{img_idx}.png");
    let out_path = media_dir.join(&filename);

    dyn_img
        .save(&out_path)
        .map_err(|e| format!("Save image: {e}"))?;

    let file_size = std::fs::metadata(&out_path)
        .map(|m| m.len() as usize)
        .unwrap_or(0);
    if file_size < MIN_IMAGE_BYTES {
        let _ = std::fs::remove_file(&out_path);
        return Ok(None);
    }

    let caption = if context.is_empty() {
        format!("(slide {slide_num} image)")
    } else {
        context.chars().take(400).collect()
    };

    Ok(Some(ParsedElement::image(
        caption,
        out_path,
        format!("slide:{slide_num}:image:{img_idx}"),
    )))
}

/// Render a table as a simple grid PNG image (same approach as DOCX tables).
fn render_pptx_table(
    rows: &[Vec<String>],
    media_dir: &Path,
    slide_num: usize,
    table_idx: u32,
    context: &str,
) -> Result<Option<ParsedElement>, String> {
    use image::{DynamicImage, Rgb, RgbImage};

    if rows.is_empty() {
        return Ok(None);
    }

    let cell_padding = 8u32;
    let char_width = 7u32;
    let row_height = 24u32;
    let line_color = Rgb([80, 80, 80]);
    let bg_color = Rgb([255, 255, 255]);
    let header_bg = Rgb([230, 230, 240]);

    let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut col_widths: Vec<u32> = vec![60; num_cols];

    for row in rows {
        for (j, cell) in row.iter().enumerate() {
            if j < col_widths.len() {
                let needed = (cell.len() as u32 * char_width) + cell_padding * 2;
                col_widths[j] = col_widths[j].max(needed.min(300));
            }
        }
    }

    let total_w: u32 = col_widths.iter().sum::<u32>() + (num_cols as u32 + 1);
    let total_h: u32 = (rows.len() as u32) * row_height + (rows.len() as u32 + 1);
    let w = total_w.min(2000);
    let h = total_h.min(2000);

    if w < MIN_IMAGE_WIDTH || h < MIN_IMAGE_HEIGHT {
        return Ok(None);
    }

    let mut img = RgbImage::from_pixel(w, h, bg_color);

    // Header background
    for x in 0..w {
        for y in 0..row_height.min(h) {
            img.put_pixel(x, y, header_bg);
        }
    }

    // Horizontal lines
    for i in 0..=rows.len() {
        let y = (i as u32) * (row_height + 1);
        if y < h {
            for x in 0..w {
                img.put_pixel(x, y, line_color);
            }
        }
    }

    // Vertical lines
    let mut x_off = 0u32;
    for j in 0..=num_cols {
        if x_off < w {
            for y in 0..h {
                img.put_pixel(x_off, y, line_color);
            }
        }
        if j < col_widths.len() {
            x_off += col_widths[j] + 1;
        }
    }

    let dyn_img = DynamicImage::ImageRgb8(img);

    let filename = format!("pptx_s{slide_num}_tbl{table_idx}.png");
    let out_path = media_dir.join(&filename);

    dyn_img
        .save(&out_path)
        .map_err(|e| format!("save table: {e}"))?;

    let file_size = std::fs::metadata(&out_path)
        .map(|m| m.len() as usize)
        .unwrap_or(0);
    if file_size < MIN_IMAGE_BYTES {
        let _ = std::fs::remove_file(&out_path);
        return Ok(None);
    }

    let caption = if context.is_empty() {
        format!("(slide {slide_num} table)")
    } else {
        context.chars().take(400).collect()
    };

    Ok(Some(ParsedElement::table(
        caption,
        out_path,
        format!("slide:{slide_num}:table:{table_idx}"),
    )))
}

/// Extract slide number from path like "ppt/slides/slide3.xml" -> 3
fn extract_slide_number(name: &str) -> u32 {
    name.trim_start_matches("ppt/slides/slide")
        .trim_end_matches(".xml")
        .parse()
        .unwrap_or(0)
}
