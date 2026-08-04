//! Scrittura WAV PCM 16 bit mono a 16 kHz, il formato che vuole Whisper.
//! Su macOS lo fa il codice Swift; su Windows serve qui.

use std::io::{Seek, SeekFrom, Write};

pub struct WavWriter {
    file: std::fs::File,
    data_bytes: u32,
}

impl WavWriter {
    pub fn create(path: &std::path::Path) -> std::io::Result<Self> {
        let mut file = std::fs::File::create(path)?;
        file.write_all(&header(0))?;
        Ok(Self {
            file,
            data_bytes: 0,
        })
    }

    pub fn write(&mut self, samples: &[i16]) -> std::io::Result<()> {
        if samples.is_empty() {
            return Ok(());
        }
        let mut buffer = Vec::with_capacity(samples.len() * 2);
        for campione in samples {
            buffer.extend_from_slice(&campione.to_le_bytes());
        }
        self.file.write_all(&buffer)?;
        self.data_bytes = self.data_bytes.saturating_add(buffer.len() as u32);
        Ok(())
    }

    /// L'intestazione va riscritta alla fine, quando le dimensioni sono note.
    pub fn close(&mut self) {
        let _ = self.file.seek(SeekFrom::Start(0));
        let _ = self.file.write_all(&header(self.data_bytes));
        let _ = self.file.flush();
    }
}

fn header(data_bytes: u32) -> Vec<u8> {
    const RATE: u32 = 16_000;
    let byte_rate = RATE * 2;

    let mut intestazione = Vec::with_capacity(44);
    intestazione.extend_from_slice(b"RIFF");
    intestazione.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    intestazione.extend_from_slice(b"WAVEfmt ");
    intestazione.extend_from_slice(&16_u32.to_le_bytes());
    intestazione.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    intestazione.extend_from_slice(&1_u16.to_le_bytes()); // mono
    intestazione.extend_from_slice(&RATE.to_le_bytes());
    intestazione.extend_from_slice(&byte_rate.to_le_bytes());
    intestazione.extend_from_slice(&2_u16.to_le_bytes()); // block align
    intestazione.extend_from_slice(&16_u16.to_le_bytes()); // bit depth
    intestazione.extend_from_slice(b"data");
    intestazione.extend_from_slice(&data_bytes.to_le_bytes());
    intestazione
}
