use crate::error::BoxError;
use base64::{prelude::BASE64_STANDARD, Engine};
use ring::pbkdf2;
use std::num::NonZeroU32;

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

pub fn pbkdf2_derive(salt: &str, iterations: u32, target: &str) -> Result<String, BoxError> {
    let mut out = [0; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(iterations).unwrap(),
        salt.as_bytes(),
        target.as_bytes(),
        &mut out,
    );
    Ok(BASE64_STANDARD.encode(out))
}

#[test]
fn test() {
    let target = "123456";
    let rng_str = crate::utils::token::get_rng_str(22);
    let derive_str = pbkdf2_derive(&rng_str, 720000, target).unwrap();
    println!("{}", derive_str);
    println!(
        "{:?}",
        pbkdf2_verify(
            target,
            &format!("pbkdf2_sha256$720000${}${}", rng_str, derive_str)
        )
    );
}
