//! Compression and Encryption middlewares for the transit layer.
//!
//! Mirrors `moleculer/src/middlewares/transmit/compression.js`
//! and `moleculer/src/middlewares/transmit/encryption.js`.
//!
//! These operate on raw packet bytes before/after sending over the transporter.

use crate::error::{MoleculerError, Result};

// ─── Compression ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionMethod {
    None,
    /// DEFLATE/zlib level 6 (default).
    Deflate,
}

impl Default for CompressionMethod {
    fn default() -> Self { CompressionMethod::Deflate }
}

pub struct CompressorCodec {
    pub method: CompressionMethod,
    pub level: u8,
    /// Minimum bytes before compressing (avoid overhead for tiny packets).
    pub threshold: usize,
}

impl Default for CompressorCodec {
    fn default() -> Self {
        Self { method: CompressionMethod::Deflate, level: 6, threshold: 1024 }
    }
}

impl CompressorCodec {
    pub fn new(method: CompressionMethod, level: u8) -> Self {
        Self { method, level, threshold: 1024 }
    }

    /// Compress bytes if above threshold.
    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < self.threshold || self.method == CompressionMethod::None {
            return Ok(data.to_vec());
        }
        // Simple zlib-compatible deflate using miniz (via flate2 if available, else passthrough)
        // For now, passthrough — wired up when flate2 is added as a dep.
        log::trace!("[Compressor] compress {} bytes", data.len());
        Ok(data.to_vec())
    }

    /// Decompress bytes.
    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        if self.method == CompressionMethod::None {
            return Ok(data.to_vec());
        }
        Ok(data.to_vec())
    }
}

// ─── Encryption ──────────────────────────────────────────────────────────────

/// AES-256-CBC encryption for transit packets.
///
/// Mirrors `moleculer/src/middlewares/transmit/encryption.js`.
/// In production, use a proper AES implementation (e.g., `aes` + `cbc` crates).
/// This stub provides the API contract; real encryption added via feature flag.
pub struct EncryptorCodec {
    key: Vec<u8>,
    iv_length: usize,
}

impl EncryptorCodec {
    /// Create with a 32-byte (256-bit) key.
    pub fn new(key: impl Into<Vec<u8>>) -> Self {
        let mut k = key.into();
        k.resize(32, 0); // pad/truncate to 32 bytes
        Self { key: k, iv_length: 16 }
    }

    /// Encrypt payload bytes. Returns `[IV (16 bytes) || ciphertext]`.
    pub fn encrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        // Stub: in real use, replace with AES-256-CBC encrypt
        // For now, XOR with key bytes for demonstration (NOT SECURE)
        log::trace!("[Encryptor] encrypt {} bytes", data.len());
        let mut out = vec![0u8; self.iv_length]; // fake IV
        rand::Rng::fill(&mut rand::thread_rng(), &mut out[..]);
        let cipher: Vec<u8> = data.iter().enumerate()
            .map(|(i, b)| b ^ self.key[i % self.key.len()])
            .collect();
        out.extend(cipher);
        Ok(out)
    }

    /// Decrypt payload bytes. Expects `[IV || ciphertext]`.
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() <= self.iv_length {
            return Err(MoleculerError::Transport("encrypted packet too short".into()));
        }
        let cipher = &data[self.iv_length..];
        let plain: Vec<u8> = cipher.iter().enumerate()
            .map(|(i, b)| b ^ self.key[i % self.key.len()])
            .collect();
        Ok(plain)
    }
}

// ─── Codec pipeline ──────────────────────────────────────────────────────────

/// Combined codec — compress then encrypt on send, decrypt then decompress on receive.
pub struct TransmitCodec {
    pub compressor: Option<CompressorCodec>,
    pub encryptor: Option<EncryptorCodec>,
}

impl TransmitCodec {
    pub fn new() -> Self {
        Self { compressor: None, encryptor: None }
    }

    pub fn with_compression(mut self, method: CompressionMethod, level: u8) -> Self {
        self.compressor = Some(CompressorCodec::new(method, level));
        self
    }

    pub fn with_encryption(mut self, key: impl Into<Vec<u8>>) -> Self {
        self.encryptor = Some(EncryptorCodec::new(key));
        self
    }

    pub fn encode(&self, data: &[u8]) -> Result<Vec<u8>> {
        let compressed = if let Some(c) = &self.compressor {
            c.compress(data)?
        } else {
            data.to_vec()
        };
        if let Some(e) = &self.encryptor {
            e.encrypt(&compressed)
        } else {
            Ok(compressed)
        }
    }

    pub fn decode(&self, data: &[u8]) -> Result<Vec<u8>> {
        let decrypted = if let Some(e) = &self.encryptor {
            e.decrypt(data)?
        } else {
            data.to_vec()
        };
        if let Some(c) = &self.compressor {
            c.decompress(&decrypted)
        } else {
            Ok(decrypted)
        }
    }
}

impl Default for TransmitCodec {
    fn default() -> Self { Self::new() }
}
