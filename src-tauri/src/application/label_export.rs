//! Backend-owned Label Studio export and thumbnail storage.

use crate::domain::errors::DomainError;
use base64::Engine;
use flate2::{write::ZlibEncoder, Compression};
use image::ImageReader;
use std::fs::OpenOptions;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const MAX_EXPORT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_EXPORT_PIXELS: u64 = 40_000_000;
pub const MAX_EXPORT_EDGE: u32 = 20_000;
pub const THUMBNAIL_EDGE: u32 = 320;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Pdf,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ExportReceipt {
    pub file_name: String,
    pub output_path: String,
    pub format: String,
    pub width_px: u32,
    pub height_px: u32,
    pub dpi: u32,
}

#[derive(Clone)]
pub struct LabelExportService {
    exports: PathBuf,
    labels: PathBuf,
}

impl LabelExportService {
    pub fn new(exports: PathBuf, labels: PathBuf) -> Result<Self, DomainError> {
        std::fs::create_dir_all(&exports).map_err(|_| DomainError::LabelExportIoFailed)?;
        std::fs::create_dir_all(labels.join("thumbnails"))
            .map_err(|_| DomainError::LabelExportIoFailed)?;
        Ok(Self { exports, labels })
    }

    pub fn export(
        &self,
        project_name: &str,
        width_mm: f64,
        height_mm: f64,
        dpi: u32,
        format: ExportFormat,
        data_url: &str,
    ) -> Result<ExportReceipt, DomainError> {
        let extension = if format == ExportFormat::Png {
            "png"
        } else {
            "pdf"
        };
        let file_name = unique_file_name(project_name, extension);
        self.export_to_path(
            width_mm,
            height_mm,
            dpi,
            format,
            data_url,
            &self.exports.join(&file_name),
        )
    }

    pub fn export_to_path(
        &self,
        width_mm: f64,
        height_mm: f64,
        dpi: u32,
        format: ExportFormat,
        data_url: &str,
        destination: &Path,
    ) -> Result<ExportReceipt, DomainError> {
        let expected = pixel_dimensions(width_mm, height_mm, dpi)?;
        let png = decode_png_data_url(data_url)?;
        validate_png(&png, expected.0, expected.1, false)?;
        let extension = if format == ExportFormat::Png {
            "png"
        } else {
            "pdf"
        };
        if !destination.is_absolute()
            || destination.extension().and_then(|value| value.to_str()) != Some(extension)
            || destination.file_name().is_none()
            || !destination.parent().is_some_and(Path::exists)
        {
            return Err(DomainError::LabelExportInvalid);
        }
        let output = match format {
            ExportFormat::Png => png_with_density(&png, dpi)?,
            ExportFormat::Pdf => raster_pdf(&png, width_mm, height_mm)?,
        };
        write_selected(destination, &output)?;
        let file_name = destination
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(DomainError::LabelExportInvalid)?
            .to_owned();
        Ok(ExportReceipt {
            file_name,
            output_path: destination.to_string_lossy().into_owned(),
            format: extension.into(),
            width_px: expected.0,
            height_px: expected.1,
            dpi,
        })
    }

    pub fn calibration_sheet(&self) -> Result<ExportReceipt, DomainError> {
        let file_name = unique_file_name("folha-calibracao-100mm", "pdf");
        let output_path = self.exports.join(&file_name);
        let bytes = calibration_pdf();
        write_new(&output_path, &bytes)?;
        Ok(ExportReceipt {
            file_name,
            output_path: output_path.to_string_lossy().into_owned(),
            format: "pdf".into(),
            width_px: 0,
            height_px: 0,
            dpi: 0,
        })
    }

    pub fn store_thumbnail(&self, data_url: &str) -> Result<(String, u32, u32), DomainError> {
        let png = decode_png_data_url(data_url).map_err(|_| DomainError::LabelThumbnailInvalid)?;
        let (width, height) =
            png_dimensions(&png).map_err(|_| DomainError::LabelThumbnailInvalid)?;
        if width > THUMBNAIL_EDGE || height > THUMBNAIL_EDGE {
            return Err(DomainError::LabelThumbnailInvalid);
        }
        validate_png(&png, width, height, true).map_err(|_| DomainError::LabelThumbnailInvalid)?;
        let relative = format!("thumbnails/{}.png", Uuid::now_v7());
        write_new(&self.labels.join(&relative), &png)
            .map_err(|_| DomainError::LabelThumbnailInvalid)?;
        Ok((relative, width, height))
    }

    pub fn read_thumbnail(&self, relative_path: &str) -> Result<Vec<u8>, DomainError> {
        let metadata = crate::domain::label::ThumbnailMetadata {
            relative_path: relative_path.into(),
            width_px: 1,
            height_px: 1,
            mime_type: "image/png".into(),
            updated_at: chrono::Utc::now(),
        };
        metadata.validate()?;
        std::fs::read(self.labels.join(relative_path))
            .map_err(|_| DomainError::LabelThumbnailInvalid)
    }

    pub fn remove_thumbnail(&self, relative_path: &str) {
        let metadata = crate::domain::label::ThumbnailMetadata {
            relative_path: relative_path.into(),
            width_px: 1,
            height_px: 1,
            mime_type: "image/png".into(),
            updated_at: chrono::Utc::now(),
        };
        if metadata.validate().is_ok() {
            let _ = std::fs::remove_file(self.labels.join(relative_path));
        }
    }
}

pub fn pixel_dimensions(
    width_mm: f64,
    height_mm: f64,
    dpi: u32,
) -> Result<(u32, u32), DomainError> {
    if !width_mm.is_finite()
        || !height_mm.is_finite()
        || width_mm <= 0.0
        || height_mm <= 0.0
        || !(72..=1_200).contains(&dpi)
    {
        return Err(DomainError::LabelExportInvalid);
    }
    let width = (width_mm / 25.4 * f64::from(dpi)).round();
    let height = (height_mm / 25.4 * f64::from(dpi)).round();
    if width < 1.0
        || height < 1.0
        || width > f64::from(MAX_EXPORT_EDGE)
        || height > f64::from(MAX_EXPORT_EDGE)
        || width * height > MAX_EXPORT_PIXELS as f64
    {
        return Err(DomainError::LabelExportTooLarge);
    }
    Ok((width as u32, height as u32))
}

fn decode_png_data_url(value: &str) -> Result<Vec<u8>, DomainError> {
    const PREFIX: &str = "data:image/png;base64,";
    let encoded = value
        .strip_prefix(PREFIX)
        .ok_or(DomainError::LabelExportInvalid)?;
    if encoded.len() > MAX_EXPORT_BYTES * 4 / 3 + 4 {
        return Err(DomainError::LabelExportTooLarge);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| DomainError::LabelExportInvalid)?;
    if bytes.len() > MAX_EXPORT_BYTES {
        return Err(DomainError::LabelExportTooLarge);
    }
    Ok(bytes)
}

fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32), DomainError> {
    if bytes.len() < 24 || &bytes[..8] != PNG_SIGNATURE || &bytes[12..16] != b"IHDR" {
        return Err(DomainError::LabelExportInvalid);
    }
    Ok((
        u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
        u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
    ))
}

fn validate_png(
    bytes: &[u8],
    expected_width: u32,
    expected_height: u32,
    thumbnail: bool,
) -> Result<(), DomainError> {
    let dimensions = png_dimensions(bytes)?;
    if dimensions != (expected_width, expected_height)
        || dimensions.0 == 0
        || dimensions.1 == 0
        || (!thumbnail && (u64::from(dimensions.0) * u64::from(dimensions.1) > MAX_EXPORT_PIXELS))
    {
        return Err(DomainError::LabelExportInvalid);
    }
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| DomainError::LabelExportInvalid)?;
    if reader.format() != Some(image::ImageFormat::Png) {
        return Err(DomainError::LabelExportInvalid);
    }
    reader
        .into_dimensions()
        .map_err(|_| DomainError::LabelExportInvalid)?;
    Ok(())
}

fn png_with_density(bytes: &[u8], dpi: u32) -> Result<Vec<u8>, DomainError> {
    let mut cursor = 8usize;
    let mut output = Vec::with_capacity(bytes.len() + 21);
    output.extend_from_slice(PNG_SIGNATURE);
    while cursor + 12 <= bytes.len() {
        let length = u32::from_be_bytes(
            bytes[cursor..cursor + 4]
                .try_into()
                .map_err(|_| DomainError::LabelExportInvalid)?,
        ) as usize;
        let end = cursor
            .checked_add(12 + length)
            .filter(|end| *end <= bytes.len())
            .ok_or(DomainError::LabelExportInvalid)?;
        let kind = &bytes[cursor + 4..cursor + 8];
        if kind != b"pHYs" {
            output.extend_from_slice(&bytes[cursor..end]);
        }
        cursor = end;
        if kind == b"IHDR" {
            let ppm = (f64::from(dpi) / 0.0254).round() as u32;
            append_png_chunk(
                &mut output,
                b"pHYs",
                &[&ppm.to_be_bytes()[..], &ppm.to_be_bytes()[..], &[1]].concat(),
            );
        }
        if kind == b"IEND" {
            break;
        }
    }
    if cursor != bytes.len() {
        return Err(DomainError::LabelExportInvalid);
    }
    Ok(output)
}

fn append_png_chunk(output: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    output.extend_from_slice(&(data.len() as u32).to_be_bytes());
    output.extend_from_slice(kind);
    output.extend_from_slice(data);
    let mut crc = crc32fast::Hasher::new();
    crc.update(kind);
    crc.update(data);
    output.extend_from_slice(&crc.finalize().to_be_bytes());
}

fn raster_pdf(png: &[u8], width_mm: f64, height_mm: f64) -> Result<Vec<u8>, DomainError> {
    let rgba = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|_| DomainError::LabelExportInvalid)?
        .to_rgba8();
    let (width, height) = rgba.dimensions();
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for pixel in rgba.pixels() {
        let alpha = u16::from(pixel[3]);
        for channel in &pixel.0[..3] {
            rgb.push(((u16::from(*channel) * alpha + 255 * (255 - alpha)) / 255) as u8);
        }
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&rgb)
        .map_err(|_| DomainError::LabelExportIoFailed)?;
    let compressed = encoder
        .finish()
        .map_err(|_| DomainError::LabelExportIoFailed)?;
    let width_pt = width_mm / 25.4 * 72.0;
    let height_pt = height_mm / 25.4 * 72.0;
    let content = format!("q\n{width_pt:.6} 0 0 {height_pt:.6} 0 0 cm\n/Im0 Do\nQ\n");
    Ok(build_pdf(
        width_pt,
        height_pt,
        width,
        height,
        &compressed,
        content.as_bytes(),
    ))
}

fn build_pdf(
    width_pt: f64,
    height_pt: f64,
    image_width: u32,
    image_height: u32,
    image: &[u8],
    content: &[u8],
) -> Vec<u8> {
    let objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width_pt:.6} {height_pt:.6}] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>").into_bytes(),
        [format!("<< /Type /XObject /Subtype /Image /Width {image_width} /Height {image_height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n", image.len()).into_bytes(), image.to_vec(), b"\nendstream".to_vec()].concat(),
        [format!("<< /Length {} >>\nstream\n", content.len()).into_bytes(), content.to_vec(), b"endstream".to_vec()].concat(),
    ];
    serialize_pdf(&objects)
}

fn calibration_pdf() -> Vec<u8> {
    let width_pt = 210.0 / 25.4 * 72.0;
    let height_pt = 297.0 / 25.4 * 72.0;
    let ruler = 100.0 / 25.4 * 72.0;
    let content = format!(
        "0 0 0 RG\n1 w\n72 600 m\n{:.6} 600 l\nS\n72 594 m\n72 606 l\nS\n{:.6} 594 m\n{:.6} 606 l\nS\nBT /F1 14 Tf 72 640 Td (REGUA DE CALIBRACAO - 100 mm) Tj ET\nBT /F1 11 Tf 72 620 Td (Imprima em tamanho real / 100%. Desative ajustar a pagina.) Tj ET\n",
        72.0 + ruler,
        72.0 + ruler,
        72.0 + ruler
    );
    let objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width_pt:.6} {height_pt:.6}] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>").into_bytes(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        [format!("<< /Length {} >>\nstream\n", content.len()).into_bytes(), content.into_bytes(), b"endstream".to_vec()].concat(),
    ];
    serialize_pdf(&objects)
}

fn serialize_pdf(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut output = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, object) in objects.iter().enumerate() {
        offsets.push(output.len());
        output.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        output.extend_from_slice(object);
        output.extend_from_slice(b"\nendobj\n");
    }
    let xref = output.len();
    output.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        output.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    output.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    output
}

fn safe_stem(value: &str) -> String {
    let value: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = value.trim_matches('-');
    let compact = trimmed
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if compact.is_empty() {
        "etiqueta".into()
    } else {
        compact.chars().take(64).collect()
    }
}

fn unique_file_name(name: &str, extension: &str) -> String {
    let id = Uuid::now_v7().simple().to_string();
    format!("{}-{}.{}", safe_stem(name), &id[..12], extension)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), DomainError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|_| DomainError::LabelExportIoFailed)?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| DomainError::LabelExportIoFailed)
}

fn write_selected(path: &Path, bytes: &[u8]) -> Result<(), DomainError> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|_| DomainError::LabelExportIoFailed)?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| DomainError::LabelExportIoFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, RgbaImage};
    use tempfile::tempdir;

    fn png(width: u32, height: u32) -> String {
        let mut bytes = Vec::new();
        DynamicImage::ImageRgba8(RgbaImage::new(width, height))
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    }

    #[test]
    fn dimensional_conversion_covers_supported_print_dpis() {
        for (dpi, expected) in [(96, 454), (150, 709), (300, 1417), (600, 2835)] {
            assert_eq!(
                pixel_dimensions(120.0, 120.0, dpi).unwrap(),
                (expected, expected)
            );
        }
    }

    #[test]
    fn png_export_has_signature_dimensions_and_density() {
        let root = tempdir().unwrap();
        let service =
            LabelExportService::new(root.path().join("exports"), root.path().join("labels"))
                .unwrap();
        let receipt = service
            .export(
                "Jewel Front",
                25.4,
                25.4,
                96,
                ExportFormat::Png,
                &png(96, 96),
            )
            .unwrap();
        let bytes = std::fs::read(root.path().join("exports").join(receipt.file_name)).unwrap();
        assert_eq!(&bytes[..8], PNG_SIGNATURE);
        assert_eq!(png_dimensions(&bytes).unwrap(), (96, 96));
        assert!(bytes.windows(4).any(|value| value == b"pHYs"));
        let position = bytes.windows(4).position(|value| value == b"pHYs").unwrap();
        assert_eq!(
            u32::from_be_bytes(bytes[position + 4..position + 8].try_into().unwrap()),
            3780
        );
    }

    #[test]
    fn pdf_media_box_is_exact_and_exports_are_immutable() {
        let root = tempdir().unwrap();
        let exports = root.path().join("exports");
        let service = LabelExportService::new(exports.clone(), root.path().join("labels")).unwrap();
        let first = service
            .export("Disc", 25.4, 50.8, 96, ExportFormat::Pdf, &png(96, 192))
            .unwrap();
        let second = service
            .export("Disc", 25.4, 50.8, 96, ExportFormat::Pdf, &png(96, 192))
            .unwrap();
        assert_ne!(first.file_name, second.file_name);
        let bytes = std::fs::read(exports.join(first.file_name)).unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("/MediaBox [0 0 72.000000 144.000000]"));
    }

    #[test]
    fn oversized_dimensions_and_thumbnail_are_rejected() {
        assert_eq!(
            pixel_dimensions(2_000.0, 2_000.0, 1_200),
            Err(DomainError::LabelExportTooLarge)
        );
        let root = tempdir().unwrap();
        let service =
            LabelExportService::new(root.path().join("exports"), root.path().join("labels"))
                .unwrap();
        assert_eq!(
            service.store_thumbnail(&png(321, 10)),
            Err(DomainError::LabelThumbnailInvalid)
        );
    }

    #[test]
    fn calibration_contains_physical_ruler_and_instruction() {
        let pdf = calibration_pdf();
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("355.464567"));
        assert!(text.contains("tamanho real / 100%"));
    }
}
