use std::fmt;

#[derive(Debug)]
pub enum Error {
    Eof,
    Io(std::io::Error),
    Format(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Eof => f.write_str("unexpected end of data"),
            Error::Io(e) => write!(f, "io: {e}"),
            Error::Format(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub(crate) fn bad<T>(msg: impl Into<String>) -> Result<T, Error> {
    Err(Error::Format(msg.into()))
}

pub(crate) struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    #[inline]
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.pos.checked_add(n).ok_or(Error::Eof)?;
        let s = self.data.get(self.pos..end).ok_or(Error::Eof)?;
        self.pos = end;
        Ok(s)
    }

    #[inline]
    pub fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        Ok(self.bytes(N)?.try_into().unwrap())
    }

    #[inline]
    pub fn u8(&mut self) -> Result<u8, Error> {
        let b = *self.data.get(self.pos).ok_or(Error::Eof)?;
        self.pos += 1;
        Ok(b)
    }

    #[inline]
    pub fn i16(&mut self) -> Result<i16, Error> {
        Ok(i16::from_le_bytes(self.array()?))
    }

    pub fn i32(&mut self) -> Result<i32, Error> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    pub fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn u64(&mut self) -> Result<u64, Error> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    /// .NET BinaryWriter 7-bit encoded int.
    pub fn var_u32(&mut self) -> Result<u32, Error> {
        let mut v = 0u32;
        for shift in (0..35).step_by(7) {
            let b = self.u8()?;
            v |= u32::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        bad("7-bit int too long")
    }

    /// .NET BinaryWriter string: 7-bit length prefix, UTF-8.
    pub fn string(&mut self) -> Result<String, Error> {
        let n = self.var_u32()? as usize;
        Ok(String::from_utf8_lossy(self.bytes(n)?).into_owned())
    }

    pub fn seek(&mut self, pos: usize) -> Result<(), Error> {
        if pos > self.data.len() {
            return Err(Error::Eof);
        }
        self.pos = pos;
        Ok(())
    }
}
