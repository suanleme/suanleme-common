use base64::{prelude::BASE64_STANDARD, Engine};
use ring::pbkdf2;
use std::num::NonZeroU32;

use crate::error::BoxError;

pub fn pbkdf2_verify(source: &str, target: &str) -> Result<(), BoxError> {
    let target: Vec<&str> = target.split('$').collect();
    if target.len() < 4 {
        return Err("password error".into());
    }
    pbkdf2::verify(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(target[1].parse::<u32>().unwrap()).unwrap(),
        target[2].as_bytes(),
        source.as_bytes(),
        BASE64_STANDARD
            .decode(target[3].as_bytes())
            .unwrap()
            .as_slice(),
    )
    .map_err(|e| e.to_string().into())
}

#[test]
fn test() {
    println!("{:?}", pbkdf2_verify("123456", "pbkdf2_sha256$720000$y3th6IpEPZ9OUWoSnXpMrG$kntSUNtJ8vZaHnhyTrPrsadlm8ePQX7g3lhwKR/QbJY="));
}
