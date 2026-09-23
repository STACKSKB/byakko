//! Original lightweight logarithmic frequency probes for keyboard lighting.
//! Output is six rows of intensity, not an audio reconstruction or recorder.
const WINDOW: usize = 2048;
const BANDS: usize = 32;

pub struct AudioBands {
    samples: [f32; WINDOW],
    cursor: usize,
    coefficients: [f32; BANDS],
    taper: [f32; WINDOW],
    levels: [f32; BANDS],
}

impl AudioBands {
    pub fn new(sample_rate: u32) -> Result<Self, String> {
        if !(8000..=384000).contains(&sample_rate) {
            return Err("Unsupported audio sample rate".into());
        }
        let high = 12000.0f32.min(sample_rate as f32 * 0.45);
        let coefficients = std::array::from_fn(|index| {
            let frequency = 60.0 * (high / 60.0).powf(index as f32 / (BANDS - 1) as f32);
            2.0 * (std::f32::consts::TAU * frequency / sample_rate as f32).cos()
        });
        Ok(Self {
            samples: [0.0; WINDOW],
            cursor: 0,
            coefficients,
            taper: std::array::from_fn(|i| {
                0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (WINDOW - 1) as f32).cos()
            }),
            levels: [0.0; BANDS],
        })
    }

    pub fn push(&mut self, samples: &[f32]) {
        for &sample in samples {
            self.samples[self.cursor] = if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            self.cursor = (self.cursor + 1) % WINDOW;
        }
    }

    pub fn silence(&mut self, count: usize) {
        for _ in 0..count.min(WINDOW) {
            self.samples[self.cursor] = 0.0;
            self.cursor = (self.cursor + 1) % WINDOW;
        }
    }

    pub fn frame(&mut self) -> [u8; BANDS] {
        std::array::from_fn(|band| {
            let coefficient = self.coefficients[band];
            let (mut previous, mut older) = (0.0f32, 0.0f32);
            for i in 0..WINDOW {
                let sample = self.samples[(self.cursor + i) % WINDOW] * self.taper[i];
                let next = sample + coefficient * previous - older;
                older = previous;
                previous = next;
            }
            let power =
                (previous * previous + older * older - coefficient * previous * older).max(0.0);
            let magnitude = power.sqrt() * 4.0 / WINDOW as f32;
            let db = 20.0 * magnitude.max(1e-6).log10();
            let target = ((db + 60.0) / 48.0).clamp(0.0, 1.0) * 6.0;
            self.levels[band] = target.max(self.levels[band] - 0.7);
            self.levels[band].round().clamp(0.0, 6.0) as u8
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silence_is_dark_and_tone_peaks_at_the_expected_probe() {
        let mut bands = AudioBands::new(48000).unwrap();
        assert_eq!(bands.frame(), [0; 32]);
        let index = 16;
        let frequency = 60.0 * 200.0f32.powf(index as f32 / 31.0);
        let tone: Vec<_> = (0..WINDOW)
            .map(|i| 0.3 * (std::f32::consts::TAU * frequency * i as f32 / 48000.0).sin())
            .collect();
        bands.push(&tone);
        let frame = bands.frame();
        assert_eq!(frame[index], 6);
        assert!(frame[0] < frame[index]);
        assert!(frame[31] < frame[index]);
        bands.silence(WINDOW);
        for _ in 0..12 {
            bands.frame();
        }
        assert_eq!(bands.frame(), [0; 32]);
    }
    #[test]
    fn rejects_bad_rates_and_contains_nonfinite_samples() {
        assert!(AudioBands::new(0).is_err());
        let mut bands = AudioBands::new(44100).unwrap();
        bands.push(&[f32::NAN, f32::INFINITY, f32::NEG_INFINITY]);
        assert_eq!(bands.frame(), [0; 32]);
    }
}
