//! Each protocol version's own byte form. A signed message is `version ‖ header ‖ body ‖ extensions ‖ signer ‖
//! signature`; decoding reads the version first and parses the body with that version's types, so a release can add
//! a version with a different body shape and still read every older one.
//!
//! The rest of the code works on one model ([`Message`]). Each version converts its body to the model (`upcast`) and
//! back (`downcast`, which refuses what the version can't say); signatures are checked over the version's own bytes,
//! so a record keeps verifying for as long as its version can be read. Today there is one version and it is the
//! model.

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    Error, Extensions, IdentityPublic, Message, SignedMessage,
    codec::{canonical, from_canonical, to_canonical},
    limits::MAX_MESSAGE_BYTES,
    signed::Header,
    types::Bytes64,
};

/// One protocol version's encoding of a message body.
pub(crate) trait Encoding {
    /// The version number written first in every message.
    const VERSION: u16;
    /// The body as this version encodes it.
    type Body: Serialize + DeserializeOwned;
    /// The model of a body this version decoded.
    fn upcast(body: Self::Body) -> Result<Message, Error>;
    /// This version's form of `body`, refused when the version can't say it.
    fn downcast(body: &Message) -> Result<Self::Body, Error>;
}

/// Protocol `v0`: open until the launch release, which freezes it.
pub(crate) struct V0;

impl Encoding for V0 {
    const VERSION: u16 = 0;
    type Body = Message;

    fn upcast(body: Message) -> Result<Message, Error> {
        Ok(body)
    }

    fn downcast(body: &Message) -> Result<Message, Error> {
        Ok(body.clone())
    }
}

/// A signed message as one version lays it out.
#[derive(Serialize, Deserialize)]
struct Frame<B> {
    version: u16,
    header: Header,
    body: B,
    extensions: Extensions,
    signer: IdentityPublic,
    signature: Bytes64,
}

/// Runs `$f::<Encoding>` for `$version`, or refuses a version this build doesn't speak.
macro_rules! by_version {
    ($version:expr, $f:ident ( $($arg:expr),* )) => {
        match $version {
            V0::VERSION => $f::<V0>($($arg),*),
            #[cfg(test)]
            tests::V1::VERSION => $f::<tests::V1>($($arg),*),
            other => Err(Error::UnsupportedVersion(other)),
        }
    };
}

/// The bytes a signed message's signature covers: `version ‖ header ‖ body ‖ extensions ‖ signer` in its version.
pub(crate) fn signing_bytes(
    version: u16,
    header: &Header,
    body: &Message,
    extensions: &Extensions,
    signer: &IdentityPublic,
) -> Result<Vec<u8>, Error> {
    fn bytes<E: Encoding>(
        header: &Header,
        body: &Message,
        extensions: &Extensions,
        signer: &IdentityPublic,
    ) -> Result<Vec<u8>, Error> {
        canonical(&(E::VERSION, header, E::downcast(body)?, extensions, signer))
    }
    by_version!(version, bytes(header, body, extensions, signer))
}

/// A signed message in its own version's bytes.
pub(crate) fn encode_frame(message: &SignedMessage) -> Result<Vec<u8>, Error> {
    fn frame<E: Encoding>(message: &SignedMessage) -> Result<Vec<u8>, Error> {
        to_canonical(
            &Frame {
                version: E::VERSION,
                header: message.header,
                body: E::downcast(&message.body)?,
                extensions: message.extensions.clone(),
                signer: message.signer,
                signature: message.signature,
            },
            MAX_MESSAGE_BYTES,
        )
    }
    by_version!(message.version, frame(message))
}

/// A signed message, its body parsed by the version it names.
pub(crate) fn decode_frame(bytes: &[u8]) -> Result<SignedMessage, Error> {
    fn frame<E: Encoding>(bytes: &[u8]) -> Result<SignedMessage, Error> {
        let frame: Frame<E::Body> = from_canonical(bytes, MAX_MESSAGE_BYTES)?;
        Ok(SignedMessage {
            version: frame.version,
            header: frame.header,
            body: E::upcast(frame.body)?,
            extensions: frame.extensions,
            signer: frame.signer,
            signature: frame.signature,
        })
    }
    let (version, _) = postcard::take_from_bytes::<u16>(bytes).map_err(|_| Error::Malformed)?;
    by_version!(version, frame(bytes))
}

/// An unsigned body in `version`'s bytes, as a keeper stores it.
pub(crate) fn encode_body_in(version: u16, body: &Message) -> Result<Vec<u8>, Error> {
    fn encode<E: Encoding>(body: &Message) -> Result<Vec<u8>, Error> {
        to_canonical(&E::downcast(body)?, MAX_MESSAGE_BYTES)
    }
    by_version!(version, encode(body))
}

/// An unsigned body stored in `version`'s bytes.
pub(crate) fn decode_body_in(version: u16, bytes: &[u8]) -> Result<Message, Error> {
    fn decode<E: Encoding>(bytes: &[u8]) -> Result<Message, Error> {
        E::upcast(from_canonical(bytes, MAX_MESSAGE_BYTES)?)
    }
    by_version!(version, decode(bytes))
}

#[cfg(test)]
mod tests {
    //! A second version, for tests only: the same messages with a byte in front of the body. It proves that decoding
    //! dispatches by version, that a record keeps verifying in its own version, and that a downcast the version can't
    //! express is refused.

    use serde::{Deserialize, Serialize};

    use super::{Encoding, decode_body_in, decode_frame, encode_body_in, encode_frame};
    use crate::{
        Error, Header, KeeperIdentity, Message, SignedMessage,
        messages::names::{NameChange, NameSubject},
    };

    #[derive(Serialize, Deserialize)]
    pub(crate) struct Body {
        marker: u8,
        message: Message,
    }

    pub(crate) struct V1;

    impl Encoding for V1 {
        const VERSION: u16 = 1;
        type Body = Body;

        fn upcast(body: Body) -> Result<Message, Error> {
            Ok(body.message)
        }

        fn downcast(body: &Message) -> Result<Body, Error> {
            // Something v1 can't say: a rename to this exact name, standing in for a field v1 dropped.
            if let Message::NameChange(change) = body
                && change.name == "only in v0"
            {
                return Err(Error::Malformed);
            }
            Ok(Body {
                marker: 0x5a,
                message: body.clone(),
            })
        }
    }

    fn signed(version: u16, name: &str) -> SignedMessage {
        let identity = KeeperIdentity::from_seed(&[3; 32]);
        let change =
            NameChange::sign(&identity, [7; 32], NameSubject::Vault, name.into(), 1).unwrap();
        let header = Header {
            scope: [7; 32],
            epoch: 0,
            id: [1; 16],
            created_at: 1,
        };
        SignedMessage::sign(&identity, version, header, Message::NameChange(change)).unwrap()
    }

    #[test]
    fn each_version_decodes_in_its_own_bytes_and_keeps_verifying() {
        for version in [0, 1] {
            let message = signed(version, "Treasury");
            let bytes = encode_frame(&message).unwrap();
            let decoded = decode_frame(&bytes).unwrap();
            assert_eq!(decoded, message);
            decoded.verify().unwrap();
        }
        // The same message is different bytes in each version, and a signature from one doesn't carry to the other.
        let v0 = signed(0, "Treasury");
        let v1 = signed(1, "Treasury");
        assert_ne!(encode_frame(&v0).unwrap(), encode_frame(&v1).unwrap());
        let moved = SignedMessage { version: 1, ..v0 };
        assert_eq!(moved.verify(), Err(Error::InvalidSignature));
    }

    #[test]
    fn a_version_refuses_what_it_cannot_say_and_an_unknown_version_is_refused() {
        let message = Message::NameChange(
            NameChange::sign(
                &KeeperIdentity::from_seed(&[3; 32]),
                [7; 32],
                NameSubject::Vault,
                "only in v0".into(),
                1,
            )
            .unwrap(),
        );
        assert!(encode_body_in(0, &message).is_ok());
        assert_eq!(encode_body_in(1, &message), Err(Error::Malformed));
        let bytes = encode_body_in(0, &message).unwrap();
        assert_eq!(decode_body_in(0, &bytes).unwrap(), message);
        assert_eq!(decode_body_in(9, &bytes), Err(Error::UnsupportedVersion(9)));

        let mut frame = encode_frame(&signed(0, "Treasury")).unwrap();
        frame[0] = 9;
        assert_eq!(decode_frame(&frame), Err(Error::UnsupportedVersion(9)));
    }
}
