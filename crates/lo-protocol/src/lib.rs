//! The game protocol of the DarkOrbit Unity client.
//!
//! This crate is the Rust counterpart of OpenOrbit's `server/protocol.py` and stays
//! byte-for-byte compatible with it (checked by the differential tests in `tests/`).
//!
//! * [`Spec`] loads the message description (`data/protocol_read.json`, extracted from the client).
//! * [`Spec::decode`] / [`Spec::encode`] turn a message payload into a [`Struct`] and back.
//! * [`frame`] and [`Framer`] handle the length-prefixed TCP framing; [`downstream`] the
//!   optional LZ4 compression the official server applied to its own messages.
//!
//! Wire format (from the client's code):
//!
//! ```text
//! client -> server : first packet 4-byte BE length (VersionRequest), then 3-byte BE length
//! server -> client : 3-byte BE length
//! payload          : u16 message id, then the fields
//! ```
//!
//! Most numeric fields are obfuscated by the client with bit rotations; each field carries the
//! expression the client's `read()` applies ([`Expr`]), and encoding applies the inverse.

mod codec;
pub mod display;
mod expr;
mod frame;
mod spec;
mod value;

pub use codec::{DecodeError, Decoded, EncodeError, MAX_DEPTH};
pub use expr::{BinOp, Expr, UnOp};
pub use frame::{Compression, FrameError, Framer, MAX_FRAME, Part, downstream, frame, lz4_block};
pub use spec::{Counter, FieldType, MessageSpec, Prim, Spec, SpecError, Step};
pub use value::{Struct, Value};

/// The `VersionRequest` value of the client `protocol_read.json` was extracted from
/// (1.1.115; the same in 1.1.116 and 1.1.117).
pub const CLIENT_HASH: &str = "0b64b1b04d103c9ec4b4f121acb7de36";

/// Id of `VersionRequest`, the first message a client sends (with a 4-byte length).
pub const VERSION_REQUEST_ID: u16 = 666;
