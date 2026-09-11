//! Compression Streams backing ops.

use deno_core::op2;
use serde::Serialize;
use std::io::{Read, Write};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressionStreamResult {
    ok: bool,
    data: Vec<u8>,
    error: String,
}

fn compress_bytes(format: &str, data: &[u8]) -> Result<Vec<u8>, String> {
    use flate2::write::{DeflateEncoder, GzEncoder, ZlibEncoder};
    use flate2::Compression;

    match format {
        "gzip" => {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(data).map_err(|e| e.to_string())?;
            encoder.finish().map_err(|e| e.to_string())
        }
        "deflate" => {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(data).map_err(|e| e.to_string())?;
            encoder.finish().map_err(|e| e.to_string())
        }
        "deflate-raw" => {
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(data).map_err(|e| e.to_string())?;
            encoder.finish().map_err(|e| e.to_string())
        }
        other => Err(format!("unsupported compression format: {other}")),
    }
}

fn decompress_bytes(format: &str, data: &[u8]) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    match format {
        "gzip" => flate2::read::GzDecoder::new(data)
            .read_to_end(&mut output)
            .map_err(|e| e.to_string())?,
        "deflate" => flate2::read::ZlibDecoder::new(data)
            .read_to_end(&mut output)
            .map_err(|e| e.to_string())?,
        "deflate-raw" => flate2::read::DeflateDecoder::new(data)
            .read_to_end(&mut output)
            .map_err(|e| e.to_string())?,
        other => return Err(format!("unsupported compression format: {other}")),
    };
    Ok(output)
}

#[op2]
#[serde]
pub fn op_compression_stream_transform(
    #[string] format: String,
    decompress: bool,
    #[buffer] data: &[u8],
) -> CompressionStreamResult {
    let result = if decompress {
        decompress_bytes(&format, data)
    } else {
        compress_bytes(&format, data)
    };
    match result {
        Ok(data) => CompressionStreamResult {
            ok: true,
            data,
            error: String::new(),
        },
        Err(error) => CompressionStreamResult {
            ok: false,
            data: Vec::new(),
            error,
        },
    }
}

deno_core::extension!(
    compression_stream_extension,
    ops = [op_compression_stream_transform],
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_standard_formats_round_trip() {
        let input = b"Compression Streams round-trip: \x00\x01\xff repeated repeated repeated";
        for format in ["gzip", "deflate", "deflate-raw"] {
            let compressed = compress_bytes(format, input).expect("compress");
            assert_ne!(compressed, input, "{format} must transform the input");
            let decoded = decompress_bytes(format, &compressed).expect("decompress");
            assert_eq!(decoded, input, "{format} round-trip");
        }
    }
}
