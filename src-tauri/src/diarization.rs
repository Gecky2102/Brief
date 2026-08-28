use std::sync::Arc;

use rustfft::{num_complex::Complex32, FftPlanner};

/// Il modello di impronta vocale vuole le stesse feature di Kaldi che WeSpeaker
/// usa in addestramento: 80 bande mel, finestre da 25 ms ogni 10 ms.
const SAMPLE_RATE: f32 = 16_000.0;
const FRAME_LENGTH: usize = 400;
const FRAME_SHIFT: usize = 160;
const MEL_BINS: usize = 80;
const FFT_SIZE: usize = 512;
const PREEMPHASIS: f32 = 0.97;

/// Sotto questa durata l'impronta vocale è troppo instabile per essere usata.
pub const MIN_SPEECH_SECONDS: f32 = 1.0;

fn hz_to_mel(hz: f32) -> f32 {
    1127.0 * (1.0 + hz / 700.0).ln()
}

fn mel_to_hz(mel: f32) -> f32 {
    700.0 * ((mel / 1127.0).exp() - 1.0)
}

/// Banco di filtri triangolari, calcolato una volta sola.
fn mel_filters() -> Vec<Vec<f32>> {
    let low = hz_to_mel(20.0);
    let high = hz_to_mel(SAMPLE_RATE / 2.0 - 400.0);
    let step = (high - low) / (MEL_BINS + 1) as f32;

    let bin_hz = SAMPLE_RATE / FFT_SIZE as f32;
    let spettro = FFT_SIZE / 2 + 1;

    (0..MEL_BINS)
        .map(|indice| {
            let sinistra = mel_to_hz(low + step * indice as f32);
            let centro = mel_to_hz(low + step * (indice + 1) as f32);
            let destra = mel_to_hz(low + step * (indice + 2) as f32);

            (0..spettro)
                .map(|bin| {
                    let hz = bin as f32 * bin_hz;
                    if hz <= sinistra || hz >= destra {
                        0.0
                    } else if hz <= centro {
                        (hz - sinistra) / (centro - sinistra)
                    } else {
                        (destra - hz) / (destra - centro)
                    }
                })
                .collect()
        })
        .collect()
}

/// Finestra di Povey, quella usata da Kaldi e quindi da WeSpeaker.
fn povey_window() -> Vec<f32> {
    (0..FRAME_LENGTH)
        .map(|i| {
            let x = 2.0 * std::f32::consts::PI * i as f32 / (FRAME_LENGTH - 1) as f32;
            (0.5 - 0.5 * x.cos()).powf(0.85)
        })
        .collect()
}

/// Calcola le feature mel logaritmiche, con la media sottratta per banda come
/// fa WeSpeaker: senza quella normalizzazione gli embedding non sono confrontabili.
pub fn fbank(samples: &[f32]) -> Vec<Vec<f32>> {
    if samples.len() < FRAME_LENGTH {
        return Vec::new();
    }

    let filtri = mel_filters();
    let finestra = povey_window();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);

    let frames = (samples.len() - FRAME_LENGTH) / FRAME_SHIFT + 1;
    let mut risultato: Vec<Vec<f32>> = Vec::with_capacity(frames);

    for indice in 0..frames {
        let inizio = indice * FRAME_SHIFT;
        let blocco = &samples[inizio..inizio + FRAME_LENGTH];

        // Rimozione della componente continua, poi preenfasi.
        let media: f32 = blocco.iter().sum::<f32>() / FRAME_LENGTH as f32;
        let mut finestrato = vec![0.0_f32; FRAME_LENGTH];
        for i in (1..FRAME_LENGTH).rev() {
            finestrato[i] = (blocco[i] - media) - PREEMPHASIS * (blocco[i - 1] - media);
        }
        finestrato[0] = (blocco[0] - media) * (1.0 - PREEMPHASIS);
        for i in 0..FRAME_LENGTH {
            finestrato[i] *= finestra[i];
        }

        let mut spettro: Vec<Complex32> = finestrato
            .iter()
            .map(|valore| Complex32::new(*valore, 0.0))
            .chain(std::iter::repeat(Complex32::new(0.0, 0.0)))
            .take(FFT_SIZE)
            .collect();
        fft.process(&mut spettro);

        let potenza: Vec<f32> = spettro[..FFT_SIZE / 2 + 1]
            .iter()
            .map(|c| c.norm_sqr())
            .collect();

        let banda: Vec<f32> = filtri
            .iter()
            .map(|filtro| {
                let energia: f32 = filtro
                    .iter()
                    .zip(potenza.iter())
                    .map(|(peso, valore)| peso * valore)
                    .sum();
                energia.max(1e-10).ln()
            })
            .collect();

        risultato.push(banda);
    }

    // Media sottratta per banda su tutta la porzione di parlato.
    if !risultato.is_empty() {
        for banda in 0..MEL_BINS {
            let media: f32 =
                risultato.iter().map(|frame| frame[banda]).sum::<f32>() / risultato.len() as f32;
            for frame in risultato.iter_mut() {
                frame[banda] -= media;
            }
        }
    }

    risultato
}

pub struct SpeakerModel {
    session: std::sync::Mutex<ort::session::Session>,
}

impl SpeakerModel {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let session = ort::session::Session::builder()
            .map_err(|cause| format!("Motore ONNX non inizializzato: {cause}"))?
            .commit_from_file(path)
            .map_err(|cause| format!("Modello delle voci non caricato: {cause}"))?;
        Ok(Self {
            session: std::sync::Mutex::new(session),
        })
    }

    /// Impronta vocale di una porzione di parlato, normalizzata a lunghezza uno
    /// così che il confronto si riduca al prodotto scalare.
    pub fn embed(&self, samples: &[f32]) -> Result<Vec<f32>, String> {
        let feature = fbank(samples);
        if feature.is_empty() {
            return Err("Porzione troppo breve per riconoscere la voce.".into());
        }

        let frames = feature.len();
        let piatte: Vec<f32> = feature.into_iter().flatten().collect();

        let tensore = ort::value::Tensor::from_array((vec![1, frames, MEL_BINS], piatte))
            .map_err(|cause| format!("Feature non convertite: {cause}"))?;

        let mut session = self
            .session
            .lock()
            .map_err(|_| "Modello delle voci non disponibile.".to_string())?;

        let uscite = session
            .run(ort::inputs!["input_features" => tensore])
            .map_err(|cause| format!("Riconoscimento della voce fallito: {cause}"))?;

        let (_, dati) = uscite[0]
            .try_extract_tensor::<f32>()
            .map_err(|cause| format!("Impronta vocale non leggibile: {cause}"))?;

        let norma = dati.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-8);
        Ok(dati.iter().map(|v| v / norma).collect())
    }
}

/// Con impronte già normalizzate la similarità è il prodotto scalare.
pub fn similarity(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Raggruppa le impronte per voce, unendo via via le due più simili finché
/// restano sopra soglia. Semplice, ma su una manciata di parlanti funziona bene
/// e non richiede di sapere in anticipo quante persone ci sono.
#[allow(dead_code)]
pub fn cluster(embeddings: &[Vec<f32>], soglia: f32, massimo: usize) -> Vec<usize> {
    if embeddings.is_empty() {
        return Vec::new();
    }

    let mut gruppo: Vec<usize> = (0..embeddings.len()).collect();
    let mut centroidi: Vec<Vec<f32>> = embeddings.to_vec();
    let mut vivi: Vec<bool> = vec![true; embeddings.len()];
    let mut quanti = embeddings.len();

    loop {
        let mut migliore = (0_usize, 0_usize, f32::MIN);
        for i in 0..centroidi.len() {
            if !vivi[i] {
                continue;
            }
            for j in (i + 1)..centroidi.len() {
                if !vivi[j] {
                    continue;
                }
                let punteggio = similarity(&centroidi[i], &centroidi[j]);
                if punteggio > migliore.2 {
                    migliore = (i, j, punteggio);
                }
            }
        }

        let (i, j, punteggio) = migliore;
        let deve_unire = punteggio >= soglia || quanti > massimo;
        if punteggio == f32::MIN || !deve_unire || quanti <= 1 {
            break;
        }

        // Fonde j dentro i e ricalcola il centroide.
        for assegnato in gruppo.iter_mut() {
            if *assegnato == j {
                *assegnato = i;
            }
        }
        let unito: Vec<f32> = centroidi[i]
            .iter()
            .zip(centroidi[j].iter())
            .map(|(a, b)| (a + b) / 2.0)
            .collect();
        let norma = unito.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-8);
        centroidi[i] = unito.iter().map(|v| v / norma).collect();
        vivi[j] = false;
        quanti -= 1;
    }

    // Rinumera i gruppi da zero in ordine di comparsa.
    let mut mappa = std::collections::HashMap::new();
    gruppo
        .iter()
        .map(|originale| {
            let prossimo = mappa.len();
            *mappa.entry(*originale).or_insert(prossimo)
        })
        .collect()
}

pub type SharedModel = Arc<SpeakerModel>;

#[cfg(test)]
mod tests {
    use super::*;

    fn tono(frequenza: f32, durata: f32) -> Vec<f32> {
        let campioni = (SAMPLE_RATE * durata) as usize;
        (0..campioni)
            .map(|i| {
                (2.0 * std::f32::consts::PI * frequenza * i as f32 / SAMPLE_RATE).sin() * 0.5
            })
            .collect()
    }

    #[test]
    fn calcola_le_feature_mel() {
        let feature = fbank(&tono(220.0, 1.0));
        assert!(feature.len() > 90, "circa cento frame in un secondo");
        assert_eq!(feature[0].len(), MEL_BINS);
    }

    #[test]
    fn niente_feature_su_audio_troppo_breve() {
        assert!(fbank(&tono(220.0, 0.01)).is_empty());
    }

    #[test]
    fn raggruppa_le_impronte_simili() {
        let voce_a = vec![1.0, 0.0, 0.0];
        let voce_a_bis = vec![0.98, 0.2, 0.0];
        let voce_b = vec![0.0, 0.0, 1.0];

        let gruppi = cluster(&[voce_a, voce_a_bis, voce_b], 0.8, 10);
        assert_eq!(gruppi[0], gruppi[1], "le due voci simili stanno insieme");
        assert_ne!(gruppi[0], gruppi[2], "la voce diversa sta a parte");
    }

    #[test]
    fn rispetta_il_numero_massimo_di_voci() {
        let impronte: Vec<Vec<f32>> = (0..6)
            .map(|i| {
                let mut v = vec![0.0; 6];
                v[i] = 1.0;
                v
            })
            .collect();

        let gruppi = cluster(&impronte, 0.99, 2);
        let distinti: std::collections::HashSet<_> = gruppi.iter().collect();
        assert!(distinti.len() <= 2, "non più di due voci");
    }
}
