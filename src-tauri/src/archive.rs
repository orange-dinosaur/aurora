//! Writing a zip, which is what an EPUB is underneath.
//!
//! The whole file is built in memory and handed back as bytes, so nothing
//! reaches the disk until the renderer has finished, and an export that fails
//! part way leaves no broken book behind.

use std::io::{self, Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// A zip being filled, one file at a time, in the order they are added.
pub struct Archive {
	writer: ZipWriter<Cursor<Vec<u8>>>,
}

impl Default for Archive {
	fn default() -> Self {
		Self {
			writer: ZipWriter::new(Cursor::new(Vec::new())),
		}
	}
}

impl Archive {
	/// Adds a file uncompressed. An EPUB's `mimetype` has to go in this way, and
	/// first, so a program can tell what the file is from its opening bytes
	/// without unzipping it.
	pub fn store(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
		self.add(name, bytes, CompressionMethod::Stored)
	}

	/// Adds a file compressed, which is how everything else goes in.
	pub fn deflate(&mut self, name: &str, bytes: &[u8]) -> io::Result<()> {
		self.add(name, bytes, CompressionMethod::Deflated)
	}

	fn add(&mut self, name: &str, bytes: &[u8], method: CompressionMethod) -> io::Result<()> {
		let options = SimpleFileOptions::default().compression_method(method);
		self.writer.start_file(name, options)?;
		self.writer.write_all(bytes)
	}

	/// The finished zip.
	pub fn finish(self) -> io::Result<Vec<u8>> {
		Ok(self.writer.finish()?.into_inner())
	}
}

#[cfg(test)]
mod tests {
	use std::io::Read;

	use zip::ZipArchive;

	use super::*;

	#[test]
	fn a_stored_first_file_is_readable_from_the_opening_bytes() {
		let mut archive = Archive::default();
		archive.store("mimetype", b"application/epub+zip").unwrap();
		archive.deflate("text.txt", b"words").unwrap();
		let bytes = archive.finish().unwrap();

		// A local file header: the signature, the method at 8, the name's length
		// at 26 and the extra field's at 28, then the name and the data.
		assert_eq!(&bytes[..4], b"PK\x03\x04");
		assert_eq!(&bytes[8..10], &[0, 0]);
		assert_eq!(&bytes[26..30], &[8, 0, 0, 0]);
		assert_eq!(&bytes[30..58], b"mimetypeapplication/epub+zip");
	}

	#[test]
	fn what_goes_in_comes_back_out() {
		let text = "word ".repeat(1000);
		let mut archive = Archive::default();
		archive.deflate("text.txt", text.as_bytes()).unwrap();
		let bytes = archive.finish().unwrap();

		let mut zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
		let mut file = zip.by_name("text.txt").unwrap();
		assert_eq!(file.compression(), CompressionMethod::Deflated);
		let mut read = String::new();
		file.read_to_string(&mut read).unwrap();
		assert_eq!(read, text);
	}
}
