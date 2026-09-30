use std::io::{self, Read};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub(super) fn from_stdin(enabled: bool) -> io::Result<Arc<AtomicBool>> {
    let flag = Arc::new(AtomicBool::new(false));
    if enabled {
        let watched = Arc::clone(&flag);
        std::thread::Builder::new()
            .name("ironlint-parent".into())
            .spawn(move || {
                watch_close(&mut io::stdin().lock(), &watched);
            })?;
    }
    Ok(flag)
}

fn watch_close(reader: &mut impl Read, flag: &AtomicBool) {
    let mut bytes = [0_u8; 256];
    loop {
        match reader.read(&mut bytes) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    flag.store(true, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opt_out_does_not_consult_stdin() {
        assert!(!from_stdin(false).unwrap().load(Ordering::Relaxed));
    }

    #[test]
    fn arbitrary_parent_bytes_are_discarded_until_eof() {
        let flag = AtomicBool::new(false);
        watch_close(&mut io::Cursor::new(vec![255_u8; 1024]), &flag);
        assert!(flag.load(Ordering::Relaxed));
    }

    #[test]
    fn interrupted_read_retries_and_read_failure_cancels() {
        struct Failing(bool);
        impl Read for Failing {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                let kind = if std::mem::replace(&mut self.0, false) {
                    io::ErrorKind::Interrupted
                } else {
                    io::ErrorKind::BrokenPipe
                };
                Err(io::Error::from(kind))
            }
        }
        let flag = AtomicBool::new(false);
        watch_close(&mut Failing(true), &flag);
        assert!(flag.load(Ordering::Relaxed));
    }
}
