//! Shared byte validation for attachments, package import, and recovery.
use crate::error::{AppError, Result};
use std::io::Cursor;
use symphonia::core::{
    codecs::{CODEC_TYPE_MP3, DecoderOptions},
    errors::Error,
    formats::FormatOptions,
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    probe::Hint,
};

pub(crate) struct MediaInfo {
    pub extension: &'static str,
    pub mime: &'static str,
}

pub(crate) fn validate(bytes: &[u8], audio: bool) -> Result<MediaInfo> {
    if bytes.is_empty() || bytes.len() > 20 * 1024 * 1024 {
        return Err(AppError::invalid(
            "Media files must contain data and be no larger than 20 MB.",
        ));
    }
    if !audio {
        let reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
        let format = reader
            .format()
            .ok_or_else(|| AppError::invalid("Choose a PNG, JPEG, WebP, or GIF image."))?;
        let (extension, mime) = match format {
            image::ImageFormat::Png => ("png", "image/png"),
            image::ImageFormat::Jpeg => ("jpg", "image/jpeg"),
            image::ImageFormat::WebP => ("webp", "image/webp"),
            image::ImageFormat::Gif => ("gif", "image/gif"),
            _ => return Err(AppError::invalid("Choose a PNG, JPEG, WebP, or GIF image.")),
        };
        let (w, h) = reader
            .into_dimensions()
            .map_err(|_| AppError::invalid("This image is damaged."))?;
        if w == 0 || h == 0 || w as u64 * h as u64 > 40_000_000 {
            return Err(AppError::invalid(
                "Images must contain at most 40 million pixels.",
            ));
        }
        image::load_from_memory_with_format(bytes, format)
            .map_err(|_| AppError::invalid("This image cannot be decoded."))?;
        return Ok(MediaInfo { extension, mime });
    }
    let wav = bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE");
    if wav && bytes.len() >= 8 {
        let size = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        if size.checked_add(8) != Some(bytes.len()) {
            return Err(AppError::invalid("This WAV file is incomplete or damaged."));
        }
    }
    let source = MediaSourceStream::new(
        Box::new(Cursor::new(bytes.to_vec())),
        MediaSourceStreamOptions::default(),
    );
    let mut hint = Hint::new();
    hint.with_extension(if wav { "wav" } else { "mp3" });
    let mut format = symphonia::default::get_probe()
        .format(
            &hint,
            source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| AppError::invalid("Choose a valid MP3 or PCM WAV audio file."))?
        .format;
    let track = format
        .default_track()
        .ok_or_else(|| AppError::invalid("This file has no audio track."))?;
    // Enabled decoders contain only MP3 and PCM. The WAV demuxer may identify
    // compressed WAV codecs, which are deliberately refused here.
    if !wav && track.codec_params.codec != CODEC_TYPE_MP3 {
        return Err(AppError::invalid(
            "Only MP3 and PCM WAV audio are supported.",
        ));
    }
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions { verify: true })
        .map_err(|_| AppError::invalid("Only MP3 and PCM WAV audio are supported."))?;
    let mut frames = 0usize;
    let mut packets = 0usize;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(_) => return Err(AppError::invalid("This audio file is damaged.")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        packets += 1;
        if packets > 1_000_000 {
            return Err(AppError::invalid(
                "This audio file is too complex to decode safely.",
            ));
        }
        let buffer = decoder
            .decode(&packet)
            .map_err(|_| AppError::invalid("This audio file cannot be decoded."))?;
        if buffer.spec().channels.count() > 8 || buffer.spec().rate > 384_000 {
            return Err(AppError::invalid("This audio format is not supported."));
        }
        frames = frames.saturating_add(buffer.frames());
    }
    if frames == 0 || decoder.finalize().verify_ok == Some(false) {
        return Err(AppError::invalid("This audio file is empty or damaged."));
    }
    Ok(if wav {
        MediaInfo {
            extension: "wav",
            mime: "audio/wav",
        }
    } else {
        MediaInfo {
            extension: "mp3",
            mime: "audio/mpeg",
        }
    })
}
