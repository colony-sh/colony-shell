//! Channel serialization protocol (P5-27)
//!
//! Defines the wire format for structured data through channels.
//!
//! ## Wire Format
//!
//! Each frame consists of:
//! ```text
//! ┌──────────┬──────────┬──────────┬──────────────────┐
//! │ type_tag │ flags    │ length   │ payload          │
//! │ (1 byte) │ (1 byte) │ (4 bytes)│ (length bytes)   │
//! └──────────┴──────────┴──────────┴──────────────────┘
//! ```
//!
//! - **type_tag**: Message type identifier (1 byte)
//! - **flags**: Bit flags (1 byte) - streaming, compressed, etc.
//! - **length**: Payload length in bytes (4 bytes, big-endian u32)
//! - **payload**: JSON-encoded message data
//!
//! ## Streaming Support
//!
//! For large payloads, the STREAM flag indicates more frames follow.
//! The STREAM_END flag marks the final frame in a sequence.

use serde::{Deserialize, Serialize};

/// Maximum frame payload size (1 MB)
pub const MAX_FRAME_SIZE: u32 = 1_048_576;

/// Frame header size in bytes (type_tag + flags + length)
pub const HEADER_SIZE: usize = 6;

/// Type tags for different message types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum TypeTag {
    /// LLM conversation message
    LlmMessage = 1,
    /// Tool request
    ToolRequest = 2,
    /// Tool result
    ToolResult = 3,
    /// Diagnostic entry
    DiagEntry = 4,
    /// Context entry
    ContextEntry = 5,
    /// Raw text (no structured format)
    RawText = 6,
    /// Heartbeat / keepalive
    Ping = 7,
    /// Stream data chunk
    StreamChunk = 8,
}

impl TypeTag {
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::LlmMessage),
            2 => Some(Self::ToolRequest),
            3 => Some(Self::ToolResult),
            4 => Some(Self::DiagEntry),
            5 => Some(Self::ContextEntry),
            6 => Some(Self::RawText),
            7 => Some(Self::Ping),
            8 => Some(Self::StreamChunk),
            _ => None,
        }
    }
}

/// Frame flags (bit field)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameFlags(u8);

impl FrameFlags {
    pub const NONE: Self = Self(0);
    /// This frame is part of a stream (more frames follow)
    pub const STREAM: Self = Self(1 << 0);
    /// This is the final frame in a stream
    pub const STREAM_END: Self = Self(1 << 1);
    /// Payload is compressed (reserved for future use)
    pub const COMPRESSED: Self = Self(1 << 2);

    pub fn new(bits: u8) -> Self {
        Self(bits)
    }

    pub fn bits(self) -> u8 {
        self.0
    }

    pub fn has(self, flag: Self) -> bool {
        (self.0 & flag.0) != 0
    }

    pub fn set(&mut self, flag: Self) {
        self.0 |= flag.0;
    }

    pub fn is_streaming(self) -> bool {
        self.has(Self::STREAM)
    }

    pub fn is_stream_end(self) -> bool {
        self.has(Self::STREAM_END)
    }
}

/// A single frame in the channel wire protocol
#[derive(Debug, Clone)]
pub struct Frame {
    pub type_tag: TypeTag,
    pub flags: FrameFlags,
    pub payload: Vec<u8>,
}

impl Frame {
    /// Create a new frame
    pub fn new(type_tag: TypeTag, payload: Vec<u8>) -> Self {
        Self {
            type_tag,
            flags: FrameFlags::NONE,
            payload,
        }
    }

    /// Create a streaming frame
    pub fn streaming(type_tag: TypeTag, payload: Vec<u8>, is_end: bool) -> Self {
        let mut flags = FrameFlags::NONE;
        if is_end {
            flags.set(FrameFlags::STREAM_END);
        } else {
            flags.set(FrameFlags::STREAM);
        }
        Self {
            type_tag,
            flags,
            payload,
        }
    }

    /// Encode frame to bytes (header + payload)
    pub fn encode(&self) -> Vec<u8> {
        let len = self.payload.len() as u32;
        let mut buf = Vec::with_capacity(HEADER_SIZE + self.payload.len());
        buf.push(self.type_tag as u8);
        buf.push(self.flags.bits());
        buf.extend_from_slice(&len.to_be_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Decode a frame from bytes
    ///
    /// Returns the frame and the number of bytes consumed,
    /// or None if insufficient data.
    pub fn decode(data: &[u8]) -> Result<Option<(Self, usize)>, ProtocolError> {
        if data.len() < HEADER_SIZE {
            return Ok(None);
        }

        let type_tag = TypeTag::from_byte(data[0])
            .ok_or(ProtocolError::InvalidTypeTag(data[0]))?;
        let flags = FrameFlags::new(data[1]);
        let len = u32::from_be_bytes([data[2], data[3], data[4], data[5]]);

        if len > MAX_FRAME_SIZE {
            return Err(ProtocolError::FrameTooLarge(len));
        }

        let total = HEADER_SIZE + len as usize;
        if data.len() < total {
            return Ok(None);
        }

        let payload = data[HEADER_SIZE..total].to_vec();
        let frame = Self {
            type_tag,
            flags,
            payload,
        };

        Ok(Some((frame, total)))
    }
}

/// Protocol errors
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("invalid type tag: {0}")]
    InvalidTypeTag(u8),

    #[error("frame too large: {0} bytes (max: {})", MAX_FRAME_SIZE)]
    FrameTooLarge(u32),

    #[error("JSON decode error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Codec for encoding/decoding typed messages through channels
pub struct ChannelCodec {
    buffer: Vec<u8>,
}

impl ChannelCodec {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Encode a serializable value into a frame
    pub fn encode_message<T: Serialize>(
        &self,
        type_tag: TypeTag,
        value: &T,
    ) -> Result<Vec<u8>, ProtocolError> {
        let json = serde_json::to_vec(value)?;
        let frame = Frame::new(type_tag, json);
        Ok(frame.encode())
    }

    /// Encode a large payload as streaming frames
    pub fn encode_streaming<T: Serialize>(
        &self,
        type_tag: TypeTag,
        value: &T,
        chunk_size: usize,
    ) -> Result<Vec<Vec<u8>>, ProtocolError> {
        let json = serde_json::to_vec(value)?;
        let mut frames = Vec::new();

        if json.len() <= chunk_size {
            frames.push(Frame::new(type_tag, json).encode());
            return Ok(frames);
        }

        let chunks: Vec<&[u8]> = json.chunks(chunk_size).collect();
        let last_idx = chunks.len() - 1;

        for (i, chunk) in chunks.into_iter().enumerate() {
            let is_end = i == last_idx;
            let frame = Frame::streaming(type_tag, chunk.to_vec(), is_end);
            frames.push(frame.encode());
        }

        Ok(frames)
    }

    /// Feed raw bytes and try to decode frames
    pub fn feed(&mut self, data: &[u8]) -> Result<Vec<Frame>, ProtocolError> {
        self.buffer.extend_from_slice(data);
        let mut frames = Vec::new();

        loop {
            match Frame::decode(&self.buffer)? {
                Some((frame, consumed)) => {
                    self.buffer = self.buffer[consumed..].to_vec();
                    frames.push(frame);
                }
                None => break,
            }
        }

        Ok(frames)
    }

    /// Decode a frame's payload as a typed message
    pub fn decode_payload<T: for<'de> Deserialize<'de>>(
        frame: &Frame,
    ) -> Result<T, ProtocolError> {
        Ok(serde_json::from_slice(&frame.payload)?)
    }
}

impl Default for ChannelCodec {
    fn default() -> Self {
        Self::new()
    }
}

/// Accumulates streaming frames into a complete payload
pub struct StreamAssembler {
    chunks: Vec<Vec<u8>>,
    type_tag: Option<TypeTag>,
    complete: bool,
}

impl StreamAssembler {
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            type_tag: None,
            complete: false,
        }
    }

    /// Feed a frame into the assembler
    ///
    /// Returns true when the stream is complete.
    pub fn feed(&mut self, frame: Frame) -> bool {
        if self.type_tag.is_none() {
            self.type_tag = Some(frame.type_tag);
        }

        self.chunks.push(frame.payload);

        if frame.flags.is_stream_end() || !frame.flags.is_streaming() {
            self.complete = true;
        }

        self.complete
    }

    /// Get the assembled payload (only valid after complete)
    pub fn assemble(&self) -> Option<Vec<u8>> {
        if !self.complete {
            return None;
        }
        let total: usize = self.chunks.iter().map(|c| c.len()).sum();
        let mut buf = Vec::with_capacity(total);
        for chunk in &self.chunks {
            buf.extend_from_slice(chunk);
        }
        Some(buf)
    }

    /// Get the type tag (if any frames have been fed)
    pub fn type_tag(&self) -> Option<TypeTag> {
        self.type_tag
    }

    /// Check if stream is complete
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Reset the assembler for reuse
    pub fn reset(&mut self) {
        self.chunks.clear();
        self.type_tag = None;
        self.complete = false;
    }
}

impl Default for StreamAssembler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_encode_decode() {
        let frame = Frame::new(TypeTag::LlmMessage, b"hello world".to_vec());
        let encoded = frame.encode();

        let (decoded, consumed) = Frame::decode(&encoded).unwrap().unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded.type_tag, TypeTag::LlmMessage);
        assert_eq!(decoded.flags, FrameFlags::NONE);
        assert_eq!(decoded.payload, b"hello world");
    }

    #[test]
    fn test_frame_decode_insufficient_data() {
        let result = Frame::decode(&[1, 0, 0]).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_frame_decode_insufficient_payload() {
        let frame = Frame::new(TypeTag::RawText, b"hello".to_vec());
        let encoded = frame.encode();
        let partial = &encoded[..HEADER_SIZE + 2];
        let result = Frame::decode(partial).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_frame_decode_invalid_type_tag() {
        let data = vec![255, 0, 0, 0, 0, 0];
        let result = Frame::decode(&data);
        assert!(matches!(result, Err(ProtocolError::InvalidTypeTag(255))));
    }

    #[test]
    fn test_frame_too_large() {
        let mut data = vec![1, 0];
        data.extend_from_slice(&(MAX_FRAME_SIZE + 1).to_be_bytes());
        let result = Frame::decode(&data);
        assert!(matches!(result, Err(ProtocolError::FrameTooLarge(_))));
    }

    #[test]
    fn test_type_tag_roundtrip() {
        for tag in [
            TypeTag::LlmMessage,
            TypeTag::ToolRequest,
            TypeTag::ToolResult,
            TypeTag::DiagEntry,
            TypeTag::ContextEntry,
            TypeTag::RawText,
            TypeTag::Ping,
            TypeTag::StreamChunk,
        ] {
            let byte = tag as u8;
            assert_eq!(TypeTag::from_byte(byte), Some(tag));
        }
    }

    #[test]
    fn test_frame_flags() {
        let mut flags = FrameFlags::NONE;
        assert!(!flags.is_streaming());
        assert!(!flags.is_stream_end());

        flags.set(FrameFlags::STREAM);
        assert!(flags.is_streaming());

        flags.set(FrameFlags::STREAM_END);
        assert!(flags.is_stream_end());
    }

    #[test]
    fn test_codec_encode_decode() {
        let codec = ChannelCodec::new();

        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct Msg {
            text: String,
        }

        let msg = Msg {
            text: "hello".to_string(),
        };

        let bytes = codec.encode_message(TypeTag::LlmMessage, &msg).unwrap();
        let (frame, _) = Frame::decode(&bytes).unwrap().unwrap();

        let decoded: Msg = ChannelCodec::decode_payload(&frame).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_codec_feed_multiple_frames() {
        let codec = ChannelCodec::new();
        let mut decoder = ChannelCodec::new();

        let frame1 = codec
            .encode_message(TypeTag::RawText, &"first")
            .unwrap();
        let frame2 = codec
            .encode_message(TypeTag::RawText, &"second")
            .unwrap();

        let mut combined = frame1;
        combined.extend_from_slice(&frame2);

        let frames = decoder.feed(&combined).unwrap();
        assert_eq!(frames.len(), 2);
    }

    #[test]
    fn test_codec_partial_feed() {
        let codec = ChannelCodec::new();
        let mut decoder = ChannelCodec::new();

        let bytes = codec
            .encode_message(TypeTag::RawText, &"test payload")
            .unwrap();

        let mid = bytes.len() / 2;

        let frames1 = decoder.feed(&bytes[..mid]).unwrap();
        assert!(frames1.is_empty());

        let frames2 = decoder.feed(&bytes[mid..]).unwrap();
        assert_eq!(frames2.len(), 1);
    }

    #[test]
    fn test_streaming_encode() {
        let codec = ChannelCodec::new();
        let large_data = "x".repeat(100);

        let frames = codec
            .encode_streaming(TypeTag::StreamChunk, &large_data, 30)
            .unwrap();

        assert!(frames.len() > 1);

        let mut assembler = StreamAssembler::new();
        for frame_bytes in &frames {
            let (frame, _) = Frame::decode(frame_bytes).unwrap().unwrap();
            assembler.feed(frame);
        }

        assert!(assembler.is_complete());
        let payload = assembler.assemble().unwrap();
        let decoded: String = serde_json::from_slice(&payload).unwrap();
        assert_eq!(decoded, large_data);
    }

    #[test]
    fn test_streaming_single_frame_fits() {
        let codec = ChannelCodec::new();
        let frames = codec
            .encode_streaming(TypeTag::StreamChunk, &"small", 1000)
            .unwrap();

        assert_eq!(frames.len(), 1);
    }

    #[test]
    fn test_stream_assembler_reset() {
        let mut assembler = StreamAssembler::new();
        let frame = Frame::new(TypeTag::RawText, b"data".to_vec());
        assembler.feed(frame);
        assert!(assembler.is_complete());

        assembler.reset();
        assert!(!assembler.is_complete());
        assert!(assembler.type_tag().is_none());
    }
}
