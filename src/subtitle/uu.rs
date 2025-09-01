//! Use the `data_encoding` crate to provide an implementation of Aegisub's UUEncode/Decode

use data_encoding_macro::new_encoding;

const UU: data_encoding::Encoding = new_encoding! {
    symbols: r##"!"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\]^_`"##,
    padding: None,
};

pub(super) fn encode(input: &[u8]) -> String {
    // First, pad with zero bytes
    let mut padded: Vec<u8> = vec![];
    padded.extend_from_slice(input);
    let remainder = padded.len() % 3;
    if remainder != 0 {
        padded.resize(padded.len() + 3 - remainder, 0);
    }

    // Encode
    let mut result = UU.encode(&padded);

    // Remove trailing characters caused by the padding
    let blocks = input.len() / 3;
    let trail = input.len() % 3;
    result.truncate(blocks * 4 + trail + 1);

    result
}

pub(super) fn decode(input: &str) -> Result<Vec<u8>, data_encoding::DecodeError> {
    UU.decode(input.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aegi_short_blobs() {
        let mut data: Vec<u8> = vec![120];
        assert_eq!(encode(&data), "?!");
        data.push(121);
        assert_eq!(encode(&data), "?(E");
        data.push(122);
        assert_eq!(encode(&data), "?(F[");
    }

    #[test]
    fn aegi_short_strings() -> Result<(), data_encoding::DecodeError> {
        let mut data: Vec<u8> = vec![120];
        assert_eq!(decode("?!")?, data);
        data.push(121);
        assert_eq!(decode("?(E")?, data);
        data.push(122);
        assert_eq!(decode("?(F[")?, data);

        Ok(())
    }
}
