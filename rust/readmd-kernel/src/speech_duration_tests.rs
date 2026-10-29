use super::*;

fn chunk(id: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut bytes = id.to_vec();
    bytes.extend((payload.len() as u32).to_le_bytes());
    bytes.extend(payload);
    if payload.len() % 2 != 0 { bytes.push(0); }
    bytes
}

fn format(tag: u16, channels: u16, rate: u32, bits: u16) -> Vec<u8> {
    let align = channels * (bits / 8);
    let mut bytes = tag.to_le_bytes().to_vec();
    bytes.extend(channels.to_le_bytes()); bytes.extend(rate.to_le_bytes());
    bytes.extend((rate * u32::from(align)).to_le_bytes());
    bytes.extend(align.to_le_bytes()); bytes.extend(bits.to_le_bytes());
    bytes
}

fn wave(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((4u32 + chunks.iter().map(|chunk| chunk.len() as u32).sum::<u32>()).to_le_bytes());
    bytes.extend(b"WAVE");
    for chunk in chunks { bytes.extend(chunk); }
    bytes
}

fn duration(bytes: &[u8]) -> Result<Option<f64>, String> {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(bytes).unwrap();
    wave_duration(file.path())
}

fn invalid(bytes: &[u8]) {
    assert_eq!(duration(bytes).unwrap_err(), "audio_decode_failed: invalid_wave_duration");
}

#[test]
fn complete_pcm_frames_include_silence_and_stereo_channels() {
    // 132300 stereo frames at 44100 Hz are three seconds. Every frame is
    // silent; counting phrases or individual channel samples is incorrect.
    let bytes = wave(&[chunk(b"fmt ", &format(1, 2, 44100, 16)), chunk(b"data", &vec![0; 132300 * 4])]);
    assert_eq!(duration(&bytes).unwrap(), Some(3.0));
    let one_frame = wave(&[chunk(b"fmt ", &format(1, 1, 16000, 16)), chunk(b"data", &[0, 0])]);
    assert_eq!(duration(&one_frame).unwrap(), Some(1.0 / 16000.0));
    let empty = wave(&[chunk(b"fmt ", &format(1, 1, 16000, 16)), chunk(b"data", &[])]);
    assert_eq!(duration(&empty).unwrap(), Some(0.0));
}

#[test]
fn odd_metadata_and_noncanonical_chunk_order_do_not_change_frame_count() {
    let bytes = wave(&[chunk(b"JUNK", &[1, 2, 3]), chunk(b"data", &[0; 18]),
        chunk(b"LIST", b"metadata"), chunk(b"fmt ", &format(1, 1, 3, 16))]);
    assert_eq!(duration(&bytes).unwrap(), Some(3.0));
}

#[test]
fn extended_pcm_and_float_formats_use_container_frame_width() {
    for (tag, bits) in [(1u16, 24u16), (3, 32), (3, 64)] {
        let mut fmt = format(0xfffe, 2, 8000, bits);
        fmt.extend(22u16.to_le_bytes()); fmt.extend(bits.to_le_bytes());
        fmt.extend(3u32.to_le_bytes()); fmt.extend(u32::from(tag).to_le_bytes());
        fmt.extend(b"\0\0\x10\0\x80\0\0\xaa\0\x38\x9b\x71");
        assert_eq!(fmt.len(), 40);
        let bytes = wave(&[chunk(b"fmt ", &fmt), chunk(b"data", &vec![0; 8000 * 2 * usize::from(bits / 8)])]);
        assert_eq!(duration(&bytes).unwrap(), Some(1.0));
    }
    let float = wave(&[chunk(b"fmt ", &format(3, 1, 8000, 32)), chunk(b"data", &vec![0; 32000])]);
    assert_eq!(duration(&float).unwrap(), Some(1.0));
}

#[test]
fn unsupported_container_and_legal_compressed_wav_keep_recognizer_compatibility() {
    let pcm = wave(&[chunk(b"fmt ", &format(1, 1, 8000, 16)), chunk(b"data", &[0; 16])]);
    for signature in [b"RF64", b"RIFX"] {
        let mut bytes = pcm.clone(); bytes[..4].copy_from_slice(signature);
        assert_eq!(duration(&bytes).unwrap(), None);
    }
    let mulaw = wave(&[chunk(b"fmt ", &format(7, 1, 8000, 8)), chunk(b"data", &[0xff; 8000])]);
    assert_eq!(duration(&mulaw).unwrap(), None);
    let packed = wave(&[chunk(b"fmt ", &format(1, 1, 8000, 12)), chunk(b"data", &[0; 8000])]);
    assert_eq!(duration(&packed).unwrap(), None);
    let wavl = wave(&[chunk(b"fmt ", &format(1, 1, 8000, 16)), chunk(b"LIST", b"wavl")]);
    assert_eq!(duration(&wavl).unwrap(), None);
    assert_eq!(duration(b"unknown container").unwrap(), None);
}

#[test]
fn truncated_pcm_container_chunk_and_frame_are_rejected() {
    let pcm = wave(&[chunk(b"fmt ", &format(1, 1, 16000, 16)), chunk(b"data", &[0; 16])]);
    let mut truncated = pcm.clone(); truncated.pop(); invalid(&truncated);
    let mut declaration = pcm.clone(); declaration[40..44].copy_from_slice(&u32::MAX.to_le_bytes()); assert_eq!(duration(&declaration).unwrap(), None);
    let mut riff_declaration = pcm.clone(); riff_declaration[4..8].copy_from_slice(&u32::MAX.to_le_bytes()); assert_eq!(duration(&riff_declaration).unwrap(), None);
    let mut large_declaration = pcm.clone(); large_declaration[40..44].copy_from_slice(&(u32::MAX - 1).to_le_bytes()); invalid(&large_declaration);
    let partial_frame = wave(&[chunk(b"fmt ", &format(1, 2, 16000, 16)), chunk(b"data", &[0; 6])]); invalid(&partial_frame);
}

#[test]
fn zero_rates_inconsistent_alignment_and_overflowing_byte_rates_are_rejected() {
    for (offset, replacement) in [(4, 0u32.to_le_bytes()), (8, 1u32.to_le_bytes())] {
        let mut fmt = format(1, 1, 16000, 16); fmt[offset..offset + 4].copy_from_slice(&replacement);
        invalid(&wave(&[chunk(b"fmt ", &fmt), chunk(b"data", &[0; 16])]));
    }
    let mut fmt = format(1, 2, 16000, 16); fmt[12..14].copy_from_slice(&2u16.to_le_bytes());
    invalid(&wave(&[chunk(b"fmt ", &fmt), chunk(b"data", &[0; 16])]));
    let mut fmt = format(1, 2, 16000, 16); fmt[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    invalid(&wave(&[chunk(b"fmt ", &fmt), chunk(b"data", &[0; 16])]));
}

#[test]
fn duplicate_data_and_chunk_budget_do_not_produce_a_guessed_duration() {
    let fmt = chunk(b"fmt ", &format(1, 1, 16000, 16));
    assert_eq!(duration(&wave(&[fmt.clone(), chunk(b"data", &[0; 16]), chunk(b"data", &[0; 16])])).unwrap(), None);
    let mut chunks = vec![fmt];
    chunks.extend((0..4096).map(|_| chunk(b"JUNK", &[])));
    chunks.push(chunk(b"data", &[0; 16])); assert_eq!(duration(&wave(&chunks)).unwrap(), None);
}

#[test]
fn cancellation_precedes_duration_io_and_restores_the_calling_thread() {
    let result = with_cancel(std::sync::Arc::new(|| true), || wave_duration(Path::new("/readmd/nonexistent/audio.wav")));
    assert_eq!(result.unwrap_err(), "cancelled"); assert!(!cancelled());
}

#[test]
fn cancellation_interrupts_metadata_traversal() {
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    let checks = Arc::new(AtomicUsize::new(0));
    let observed = checks.clone();
    let bytes = wave(&[chunk(b"fmt ", &format(1, 1, 16000, 16)), chunk(b"JUNK", &[1]), chunk(b"data", &[0; 16])]);
    let result = with_cancel(Arc::new(move || observed.fetch_add(1, Ordering::SeqCst) >= 2), || duration(&bytes));
    assert_eq!(result.unwrap_err(), "cancelled"); assert!(checks.load(Ordering::SeqCst) >= 3); assert!(!cancelled());
}
