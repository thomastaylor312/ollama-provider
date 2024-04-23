pub struct InputStreamReader<'a> {
    stream: &'a mut crate::wasi::io::streams::InputStream,
}

impl<'a> From<&'a mut crate::wasi::io::streams::InputStream> for InputStreamReader<'a> {
    fn from(stream: &'a mut crate::wasi::io::streams::InputStream) -> Self {
        Self { stream }
    }
}

impl std::io::Read for InputStreamReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use crate::wasi::io::streams::StreamError;
        use std::io;

        let n = buf
            .len()
            .try_into()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        match self.stream.blocking_read(n) {
            Ok(chunk) => {
                let n = chunk.len();
                if n > buf.len() {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        "more bytes read than requested",
                    ));
                }
                buf[..n].copy_from_slice(&chunk);
                Ok(n)
            }
            Err(StreamError::Closed) => Ok(0),
            Err(StreamError::LastOperationFailed(e)) => {
                Err(io::Error::new(io::ErrorKind::Other, e.to_debug_string()))
            }
        }
    }
}

pub struct OutputStreamWriter<'a> {
    stream: &'a mut crate::wasi::io::streams::OutputStream,
}

impl<'a> From<&'a mut crate::wasi::io::streams::OutputStream> for OutputStreamWriter<'a> {
    fn from(stream: &'a mut crate::wasi::io::streams::OutputStream) -> Self {
        Self { stream }
    }
}

impl std::io::Write for OutputStreamWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        use crate::wasi::io::streams::StreamError;
        use std::io;

        let n = match self.stream.check_write().map(std::num::NonZeroU64::new) {
            Ok(Some(n)) => n,
            Ok(None) | Err(StreamError::Closed) => return Ok(0),
            Err(StreamError::LastOperationFailed(e)) => {
                return Err(io::Error::new(io::ErrorKind::Other, e.to_debug_string()))
            }
        };
        let n = n
            .get()
            .try_into()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let n = buf.len().min(n);
        self.stream.write(&buf[..n]).map_err(|e| match e {
            StreamError::Closed => io::ErrorKind::UnexpectedEof.into(),
            StreamError::LastOperationFailed(e) => {
                io::Error::new(io::ErrorKind::Other, e.to_debug_string())
            }
        })?;
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stream
            .blocking_flush()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}
