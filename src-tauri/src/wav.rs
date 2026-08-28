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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crea_e_scrive_wav_16k_mono() {
        let temp = std::env::temp_dir().join(format!("brief_test_{}.wav", std::process::id()));
        let mut writer = WavWriter::create(&temp).expect("creazione file");
        let samples: Vec<i16> = vec![0, 1000, -1000, 2000, -2000];
        writer.write(&samples).expect("scrittura campioni");
        writer.close();

        let bytes = std::fs::read(&temp).expect("lettura file");
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(bytes.len(), 44 + samples.len() * 2);

        let data_size = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]);
        assert_eq!(data_size as usize, samples.len() * 2);

        let _ = std::fs::remove_file(&temp);
    }
}
