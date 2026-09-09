//! Enforce the journal limit while serializing, before allocating a complete JSON
//! record or opening a temporary output file. Never expose captured file contents.
use super::*;

struct BoundedBytes {
    bytes: Vec<u8>,
    limit: u64,
    exceeded: bool,
}

impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.limit.saturating_sub(self.bytes.len() as u64) {
            self.exceeded = true;
            return Err(std::io::Error::other(
                "Preview recovery record is too large",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(super) fn encode(record: &Record, limit: u64) -> Result<Vec<u8>> {
    let mut output = BoundedBytes {
        bytes: Vec::new(),
        limit,
        exceeded: false,
    };
    let result = serde_json::to_writer(&mut output, record);
    if output.exceeded {
        return Err(invalid("Preview recovery record is too large"));
    }
    result?;
    Ok(output.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_record_json_limits_are_enforced_before_publication() {
        let home = tempfile::tempdir().unwrap();
        let env = SlateEnv::with_home(home.path().to_owned());
        let snapshot = PreviewSnapshot::capture(&env).unwrap();
        let saved = fs::read(record_path(&env)).unwrap();
        let mut record = Record::new(&snapshot, snapshot.read_current().unwrap(), false);
        record.writing = true;
        let reference = serde_json::to_vec(&record).unwrap();
        assert_eq!(encode(&record, reference.len() as u64).unwrap(), reference);
        for limit in [0, 8, reference.len() as u64 - 1] {
            let error = snapshot
                .journal
                .save_bounded(&record, limit)
                .unwrap_err()
                .to_string();
            assert!(error.contains("record is too large"));
            assert_eq!(fs::read(record_path(&env)).unwrap(), saved);
            assert_eq!(fs::read_dir(env.slate_cache_dir()).unwrap().count(), 2);
        }
        // Chunk overflow is rejected without appending even the fitting prefix.
        let mut writer = BoundedBytes {
            bytes: Vec::new(),
            limit: 3,
            exceeded: false,
        };
        writer.write_all(b"ok").unwrap();
        assert!(writer.write_all(b"large").is_err());
        assert_eq!(writer.bytes, b"ok");
        assert!(writer.exceeded);
    }
}
