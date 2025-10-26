use bech32::{ToBase32, Variant, encode};

pub fn nsec_from_sk32(sk: &[u8; 32]) -> String {
    encode("nsec", sk.to_base32(), Variant::Bech32).expect("nsec encode")
}

pub fn npub_from_xonly32(x: &[u8; 32]) -> String {
    encode("npub", x.to_base32(), Variant::Bech32).expect("npub encode")
}

