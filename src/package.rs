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
            let name = file.name().to_string();
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
