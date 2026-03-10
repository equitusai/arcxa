//! Protobuf codec for Raft types.
//!
//! Provides serialization/deserialization for Raft protobuf types (Entry, HardState, ConfState)
//! using the protobuf crate (rust-protobuf) which is what raft-proto uses.

use anyhow::Result;
use protobuf::Message;

/// Serialize a protobuf message to bytes.
///
/// This works with any type that implements `protobuf::Message`.
pub fn encode<M: Message>(msg: &M) -> Result<Vec<u8>> {
    msg.write_to_bytes()
        .map_err(|e| anyhow::anyhow!("Failed to encode protobuf message: {}", e))
}

/// Deserialize a protobuf message from bytes.
///
/// This works with any type that implements `protobuf::Message`.
pub fn decode<M: Message>(bytes: &[u8]) -> Result<M> {
    M::parse_from_bytes(bytes)
        .map_err(|e| anyhow::anyhow!("Failed to decode protobuf message: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use raft::prelude::*;

    #[test]
    fn test_encode_decode_entry() {
        let mut entry = Entry::default();
        entry.set_index(42);
        entry.set_term(10);
        entry.set_data(vec![1, 2, 3].into());

        // Encode
        let bytes = encode(&entry).unwrap();
        assert!(!bytes.is_empty());

        // Decode
        let decoded: Entry = decode(&bytes).unwrap();
        assert_eq!(decoded.index, 42);
        assert_eq!(decoded.term, 10);
        assert_eq!(decoded.data.as_ref(), &[1, 2, 3]);
    }

    #[test]
    fn test_encode_decode_hard_state() {
        let mut hs = HardState::default();
        hs.set_term(5);
        hs.set_vote(2);
        hs.set_commit(100);

        // Encode
        let bytes = encode(&hs).unwrap();
        assert!(!bytes.is_empty());

        // Decode
        let decoded: HardState = decode(&bytes).unwrap();
        assert_eq!(decoded.term, 5);
        assert_eq!(decoded.vote, 2);
        assert_eq!(decoded.commit, 100);
    }

    #[test]
    fn test_encode_decode_conf_state() {
        let mut cs = ConfState::default();
        cs.set_voters(vec![1, 2, 3]);
        cs.set_learners(vec![4, 5]);

        // Encode
        let bytes = encode(&cs).unwrap();
        assert!(!bytes.is_empty());

        // Decode
        let decoded: ConfState = decode(&bytes).unwrap();
        assert_eq!(decoded.voters, vec![1, 2, 3]);
        assert_eq!(decoded.learners, vec![4, 5]);
    }

    #[test]
    fn test_empty_entry() {
        let entry = Entry::default();

        let bytes = encode(&entry).unwrap();
        let decoded: Entry = decode(&bytes).unwrap();

        assert_eq!(decoded.index, 0);
        assert_eq!(decoded.term, 0);
        assert!(decoded.data.is_empty());
    }
}
