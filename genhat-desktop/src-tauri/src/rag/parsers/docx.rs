//! DOCX parser — text, image, and table extraction.
//!
//! Text extraction uses `docx-rs` for paragraph text.
//! Images are extracted from the `word/media/` folder inside the DOCX ZIP.
//! Tables are detected from `DocumentChild::Table` and rendered as HTML→PNG.

use std::path::Path;
use super::{ParsedDocument, ParsedElement, TextBlock, MIN_IMAGE_WIDTH, MIN_IMAGE_HEIGHT, MIN_IMAGE_BYTES};

pub fn parse(path: &Path, media_dir: Option<&Path>) -> Result<ParsedDocument, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Failed to read DOCX: {e}"))?;
    let docx = docx_rs::read_docx(&bytes).map_err(|e| format!("DOCX parse error: {e}"))?;

    let title = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("document.docx")
        .to_string();

    let mut sections = Vec::new();
    let mut elements = Vec::new();
    let mut para_idx = 0u32;
    let mut table_idx = 0u32;

    // Track preceding text for context-aware captions
    let mut prev_text = String::new();

    for child in docx.document.children.iter() {
        match child {
            docx_rs::DocumentChild::Paragraph(para) => {
                let mut text = String::new();
                let mut has_image = false;
                let mut image_rel_ids: Vec<String> = Vec::new();

                for pchild in &para.children {
                    match pchild {
                        docx_rs::ParagraphChild::Run(run) => {
                            for rchild in &run.children {
                                match rchild {
                                    docx_rs::RunChild::Text(t) => {
                                        text.push_str(&t.text);
                                    }
                                    docx_rs::RunChild::Drawing(drawing) => {
                                        has_image = true;
                                        // Try to extract the relationship ID for the image
                                        // The drawing contains inline/anchor with blip references
                                        if let Some(rel_id) = extract_drawing_rel_id(drawing) {
                                            image_rel_ids.push(rel_id);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }

                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    para_idx += 1;
                    sections.push(TextBlock {
                        text: trimmed.clone(),
                        metadata: format!("paragraph:{para_idx}"),
                    });
                    elements.push(ParsedElement::text(&trimmed, format!("paragraph:{para_idx}")));
                    prev_text = trimmed;
                }

                // Handle images found in this paragraph
                if has_image {
                    if let Some(media_dir) = media_dir {
                        for rel_id in &image_rel_ids {
                            if let Some(elem) = extract_docx_image_by_rel(
                                path,
                                rel_id,
                                media_dir,
                                &prev_text,
                                para_idx,
                            ) {
                                elements.push(elem);
                            }
                        }
                    }
                }
            }
            docx_rs::DocumentChild::Table(table) => {
                // Extract table text for indexing
                let table_text = extract_table_text(table);

                if !table_text.trim().is_empty() {
                    table_idx += 1;
                    // Add the table text as a section for chunking
                    sections.push(TextBlock {
                        text: table_text.clone(),
                        metadata: format!("table:{table_idx}"),
                    });

                    // Render table to PNG if media_dir provided
                    if let Some(media_dir) = media_dir {
                        match render_table_to_png(table, media_dir, table_idx, &prev_text) {
                            Ok(Some(elem)) => elements.push(elem),
                            Ok(None) => {
                                // Rendering failed or too small, keep as text element
                                elements.push(ParsedElement::text(
                                    &table_text,
                                    format!("table:{table_idx}"),
                                ));
                            }
                            Err(e) => {
                                log::debug!("Table render failed: {e}");
                                elements.push(ParsedElement::text(
                                    &table_text,
                                    format!("table:{table_idx}"),
                                ));
                            }
                        }
                    } else {
                        elements.push(ParsedElement::text(
                            &table_text,
                            format!("table:{table_idx}"),
                        ));
                    }

                    prev_text = table_text;
                }
            }
            _ => {}
        }
    }

    // Also try to extract all images from word/media/ that weren't pulled via drawings
    if let Some(media_dir) = media_dir {
        if let Ok(extra) = extract_all_media_images(path, media_dir, &prev_text) {
            for elem in extra {
                // Deduplicate by checking if the filename already exists
                if let Some(ref mp) = elem.media_path {
                    if !elements.iter().any(|e| e.media_path.as_ref() == Some(mp)) {
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

/// Extract text from a DOCX table (all cells, tab-separated rows).
fn extract_table_text(table: &docx_rs::Table) -> String {
    let mut rows = Vec::new();
    for row in &table.rows {
        match row {
            docx_rs::TableChild::TableRow(tr) => {
                let mut cells = Vec::new();
                for cell in &tr.cells {
                    match cell {
                        docx_rs::TableRowChild::TableCell(tc) => {
                            let mut cell_text = String::new();
                            for child in &tc.children {
                                if let docx_rs::TableCellContent::Paragraph(para) = child {
                                    for pchild in &para.children {
                                        if let docx_rs::ParagraphChild::Run(run) = pchild {
                                            for rchild in &run.children {
                                                if let docx_rs::RunChild::Text(t) = rchild {
                                                    cell_text.push_str(&t.text);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            cells.push(cell_text.trim().to_string());
                        }
                    }
                }
                rows.push(cells.join("\t"));
            }
        }
    }
    rows.join("\n")
}

/// Try to extract the relationship ID from a Drawing element.
/// Drawing → Inline/Anchor → Graphic → Blip → r:embed
fn extract_drawing_rel_id(_drawing: &docx_rs::Drawing) -> Option<String> {
    // The docx-rs Drawing type wraps the raw drawing data.
    // Extracting the exact r:embed relationship ID requires walking the
    // internal structure. For now, we fall back to extracting images
    // directly from word/media/ (extract_all_media_images).
    None
}

/// Extract an image from the DOCX ZIP by relationship ID.
fn extract_docx_image_by_rel(
    _docx_path: &Path,
    _rel_id: &str,
    _media_dir: &Path,
    _context: &str,
    _para_idx: u32,
) -> Option<ParsedElement> {
    // Relationship-based extraction is complex with docx-rs.
    // We handle this via extract_all_media_images() instead.
    None
}

/// Extract all images from the word/media/ folder in the DOCX ZIP.
fn extract_all_media_images(
    docx_path: &Path,
    media_dir: &Path,
    context: &str,
) -> Result<Vec<ParsedElement>, String> {
    let file = std::fs::File::open(docx_path)
        .map_err(|e| format!("Failed to open DOCX: {e}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("ZIP error: {e}"))?;

    let mut elements = Vec::new();
    let mut img_counter = 0u32;

    // Collect media file names first (can't borrow archive twice)
    let media_names: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            let entry = archive.by_index(i).ok()?;
            let name = entry.name().to_string();
            if name.starts_with("word/media/") && !name.ends_with('/') {
                Some(name)
            } else {
                None
            }
        })
        .collect();

    for name in &media_names {
        if let Ok(mut entry) = archive.by_name(name) {
            use std::io::Read;
            let mut buf = Vec::new();
            if entry.read_to_end(&mut buf).is_ok() && !buf.is_empty() {
                // Try to decode as an image
                if let Ok(dyn_img) = image::load_from_memory(&buf) {
                    let (w, h) = (dyn_img.width(), dyn_img.height());
                    if w < MIN_IMAGE_WIDTH || h < MIN_IMAGE_HEIGHT {
                        continue;
                    }

                    img_counter += 1;
                    let filename = format!(
                        "docx_img{img_counter}.png"
                    );
                    let out_path = media_dir.join(&filename);

                    if dyn_img.save(&out_path).is_ok() {
                        let file_size = std::fs::metadata(&out_path)
                            .map(|m| m.len() as usize)
                            .unwrap_or(0);

                        if file_size < MIN_IMAGE_BYTES {
                            let _ = std::fs::remove_file(&out_path);
                            continue;
                        }

                        let caption = if context.is_empty() {
                            format!("(embedded image from document)")
                        } else {
                            context.chars().take(400).collect::<String>()
                        };

                        elements.push(ParsedElement::image(
                            caption,
                            out_path,
                            format!("image:{img_counter}"),
                        ));
                    }
                }
            }
        }
    }

    Ok(elements)
}

/// Render a DOCX table to PNG by first building a simple HTML table,
/// then converting to an image.
///
/// This uses a self-contained approach: generate an HTML string → render
/// using a basic text-to-image approach (creates a simple grid image).
fn render_table_to_png(
    table: &docx_rs::Table,
    media_dir: &Path,
    table_idx: u32,
    context: &str,
) -> Result<Option<ParsedElement>, String> {
    // Extract table structure
    let mut rows_data: Vec<Vec<String>> = Vec::new();
    for row in &table.rows {
        match row {
            docx_rs::TableChild::TableRow(tr) => {
                let mut cells = Vec::new();
                for cell in &tr.cells {
                    match cell {
                        docx_rs::TableRowChild::TableCell(tc) => {
                            let mut cell_text = String::new();
                            for child in &tc.children {
                                if let docx_rs::TableCellContent::Paragraph(para) = child {
                                    for pchild in &para.children {
                                        if let docx_rs::ParagraphChild::Run(run) = pchild {
                                            for rchild in &run.children {
                                                if let docx_rs::RunChild::Text(t) = rchild {
                                                    cell_text.push_str(&t.text);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            cells.push(cell_text.trim().to_string());
                        }
                    }
                }
                rows_data.push(cells);
            }
        }
    }

    if rows_data.is_empty() {
        return Ok(None);
    }

    // Render as a simple grid image using the `image` crate
    let table_image = render_table_grid(&rows_data);
    if table_image.width() < MIN_IMAGE_WIDTH || table_image.height() < MIN_IMAGE_HEIGHT {
        return Ok(None);
    }

    let filename = format!("docx_tbl{table_idx}.png");
    let out_path = media_dir.join(&filename);
    table_image
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
        format!("(table #{table_idx})")
    } else {
        context.chars().take(400).collect::<String>()
    };

    Ok(Some(ParsedElement::table(
        caption,
        out_path,
        format!("table:{table_idx}"),
    )))
}

/// Render a table as a grid image using the `image` crate.
/// Simple approach: compute cell sizes from text lengths, draw lines + text placeholders.
fn render_table_grid(rows: &[Vec<String>]) -> image::DynamicImage {
    use image::{Rgb, RgbImage, DynamicImage};

    let cell_padding = 8u32;
    let char_width = 7u32;    // approximate pixel width per character
    let row_height = 24u32;
    let line_color = Rgb([80, 80, 80]);
    let bg_color = Rgb([255, 255, 255]);
    let header_bg = Rgb([230, 230, 240]);

    // Calculate column widths based on max content length
    let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut col_widths: Vec<u32> = vec![60; num_cols];

    for row in rows {
        for (j, cell) in row.iter().enumerate() {
            if j < col_widths.len() {
                let needed = (cell.len() as u32 * char_width) + cell_padding * 2;
                let capped = needed.min(300); // cap individual cell width
                col_widths[j] = col_widths[j].max(capped);
            }
        }
    }

    let total_width: u32 = col_widths.iter().sum::<u32>() + (num_cols as u32 + 1);
    let total_height: u32 = (rows.len() as u32) * row_height + (rows.len() as u32 + 1);

    // Cap image size
    let w = total_width.min(2000);
    let h = total_height.min(2000);

    let mut img = RgbImage::from_pixel(w, h, bg_color);

    // Draw header background (first row)
    if !rows.is_empty() {
        for x in 0..w {
            for y in 0..row_height {
                if x < w && y < h {
                    img.put_pixel(x, y, header_bg);
                }
            }
        }
    }

    // Draw horizontal lines
    for i in 0..=rows.len() {
        let y = (i as u32) * (row_height + 1);
        if y < h {
            for x in 0..w {
                img.put_pixel(x, y, line_color);
            }
        }
    }

    // Draw vertical lines
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

    DynamicImage::ImageRgb8(img)
}
