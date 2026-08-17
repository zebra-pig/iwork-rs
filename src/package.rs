//! The package container shared by Pages, Numbers and Keynote: a ZIP archive
//! whose entries are all *stored* (compression method 0).
//!
//! iWork relies on entries being uncompressed so media can be mapped straight
//! out of the file, and the `Index/*.iwa` payloads carry their own Snappy
//! compression, so deflating them again would only cost CPU. All three apps
//! behave identically here.

use std::io::{Cursor, Read, Write};
use zip::write::SimpleFileOptions;

use crate::Error;

/// A raw iWork package: an ordered list of ZIP entries.
///
/// Entry order is preserved on write. iWork does not appear to depend on it,
/// but keeping it makes diffs against the original file readable.
#[derive(Clone, Default)]
pub struct Package {
    pub entries: Vec<(String, Vec<u8>)>,
}

impl Package {
    pub fn read(path: impl AsRef<std::path::Path>) -> Result<Package, Error> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)?;
        Package::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Package, Error> {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
        let mut entries = Vec::with_capacity(zip.len());
        for i in 0..zip.len() {
            let mut file = zip.by_index(i)?;
            // Entry names are read as raw bytes and decoded as UTF-8, not taken
            // from `name()`.
            //
            // iWork writes them as UTF-8 but does not set the ZIP general
            // purpose flag that says so (bit 11), so a reader following the
            // spec falls back to CP437 — and a document with an umlaut in a
            // media filename comes back as mojibake. That would be a cosmetic
            // problem if the name were only a label, but `TSP.PackageMetadata`
            // refers to media by this exact name, and writing the mojibake back
            // out renames the file out from under the reference: `Schu` + U+0308
            // is stored as CC 88 and would be rewritten as E2 95 A0 C3 AA.
            // Pages then opens a document whose images have gone missing.
            let name = String::from_utf8_lossy(file.name_raw()).into_owned();
            let mut data = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut data)?;
            entries.push((name, data));
        }
        Ok(Package { entries })
    }

    pub fn write(&self, path: impl AsRef<std::path::Path>) -> Result<(), Error> {
        let file = std::fs::File::create(path)?;
        let mut zip = zip::ZipWriter::new(file);
        // Stored, never deflated — see the module comment.
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .large_file(true);
        for (name, data) in &self.entries {
            zip.start_file(name.as_str(), options)?;
            zip.write_all(data)?;
        }
        zip.finish()?;
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.entries
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, d)| d.as_slice())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.iter().any(|(n, _)| n == name)
    }

    /// Replace an entry in place, or append it if it does not exist yet.
    pub fn set(&mut self, name: &str, data: Vec<u8>) {
        match self.entries.iter_mut().find(|(n, _)| n == name) {
            Some(entry) => entry.1 = data,
            None => self.entries.push((name.to_string(), data)),
        }
    }

    pub fn remove(&mut self, name: &str) -> Option<Vec<u8>> {
        let index = self.entries.iter().position(|(n, _)| n == name)?;
        Some(self.entries.remove(index).1)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(n, _)| n.as_str())
    }

    /// Names of every object-archive stream, in package order.
    ///
    /// Numbers keeps most of its components under `Index/Tables/`, so this is
    /// not a flat listing of `Index/`.
    pub fn iwa_names(&self) -> Vec<String> {
        self.names()
            .filter(|n| n.ends_with(".iwa"))
            .map(str::to_string)
            .collect()
    }

    /// Names of media entries under `Data/`.
    pub fn data_names(&self) -> Vec<String> {
        self.names()
            .filter(|n| n.starts_with("Data/"))
            .map(str::to_string)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for byte in data {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    /// A one-entry stored ZIP with the given raw filename bytes and the UTF-8
    /// flag left clear — which is how iWork writes packages, and which the
    /// `zip` writer will not produce, so it has to be built by hand.
    fn zip_with_raw_name(name: &[u8], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let crc = crc32(data).to_le_bytes();
        let size = (data.len() as u32).to_le_bytes();
        let name_len = (name.len() as u16).to_le_bytes();

        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // version, flags, method, time
        out.extend_from_slice(&crc);
        out.extend_from_slice(&size);
        out.extend_from_slice(&size);
        out.extend_from_slice(&name_len);
        out.extend_from_slice(&[0, 0]); // extra length
        out.extend_from_slice(name);
        out.extend_from_slice(data);

        let central = out.len() as u32;
        out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        out.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        out.extend_from_slice(&crc);
        out.extend_from_slice(&size);
        out.extend_from_slice(&size);
        out.extend_from_slice(&name_len);
        out.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // extra, comment, disk, attrs
        out.extend_from_slice(&0u32.to_le_bytes()); // local header offset
        out.extend_from_slice(name);

        let central_size = out.len() as u32 - central;
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0, 1, 0, 1, 0]);
        out.extend_from_slice(&central_size.to_le_bytes());
        out.extend_from_slice(&central.to_le_bytes());
        out.extend_from_slice(&[0, 0]); // comment length
        out
    }

    fn raw_names(zip: &[u8]) -> Vec<Vec<u8>> {
        let mut archive = zip::ZipArchive::new(Cursor::new(zip)).unwrap();
        (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name_raw().to_vec())
            .collect()
    }

    /// `TSP.PackageMetadata` refers to media by exact filename, so a name must
    /// survive a read and a write byte for byte. iWork writes UTF-8 names
    /// without setting the flag that says so, and decoding those as CP437 —
    /// which is what the ZIP spec asks for — turns `ü` into two characters that
    /// re-encode to entirely different bytes, renaming the file out from under
    /// the document's own reference.
    #[test]
    fn a_utf8_name_without_the_utf8_flag_survives_a_roundtrip() {
        // "Schu" + U+0308 COMBINING DIAERESIS, exactly as macOS stores it.
        let name = b"Data/Schu\xcc\x88lerblatt Logo.png";
        let package = Package::from_bytes(&zip_with_raw_name(name, b"PNG")).unwrap();

        assert_eq!(package.names().count(), 1);
        assert_eq!(
            package.data_names(),
            vec!["Data/Schu\u{308}lerblatt Logo.png".to_string()],
            "the name decodes as the UTF-8 it is"
        );
        assert!(package.contains("Data/Schu\u{308}lerblatt Logo.png"));

        let path = std::env::temp_dir().join("iwork-package-name-test.zip");
        package.write(&path).unwrap();
        let written = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            raw_names(&written),
            vec![name.to_vec()],
            "filename bytes changed on write"
        );
        assert_eq!(
            Package::from_bytes(&written).unwrap().data_names(),
            package.data_names()
        );
    }

    #[test]
    fn ascii_names_are_unaffected() {
        let package = Package::from_bytes(&zip_with_raw_name(b"Index/Document.iwa", b"x")).unwrap();
        assert_eq!(package.iwa_names(), vec!["Index/Document.iwa".to_string()]);
    }
}
