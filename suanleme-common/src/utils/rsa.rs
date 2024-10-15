use crate::error::BoxError;
use base64::{prelude::BASE64_STANDARD, Engine};
use rand::rngs::OsRng;
use rsa::{
    pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey, EncodeRsaPrivateKey, EncodeRsaPublicKey},
    pkcs8::{DecodePrivateKey, DecodePublicKey},
    BigUint, Pkcs1v15Encrypt, Pkcs1v15Sign, RsaPrivateKey, RsaPublicKey,
};
use serde_json::Value;
use sha2::Digest;

pub fn build() -> (RsaPrivateKey, RsaPublicKey) {
    // 生成RSA密钥对
    let mut rng = OsRng;
    let bits = 2048;
    let private_key = RsaPrivateKey::new(&mut rng, bits).expect("failed to generate a key");
    let public_key = private_key.to_public_key();
    (private_key, public_key)
}
pub fn build_rsa_pair() -> (String, String) {
    // 生成RSA密钥对
    let (prik, pubk) = build();
    (
        BASE64_STANDARD.encode(prik.to_pkcs1_der().unwrap().as_bytes()),
        BASE64_STANDARD.encode(pubk.to_pkcs1_der().unwrap().as_bytes()),
    )
}

pub fn get_rsa_pubk_pkcs1_der(pubk: &str) -> Result<RsaPublicKey, BoxError> {
    RsaPublicKey::from_pkcs1_der(&BASE64_STANDARD.decode(pubk)?).map_err(|e| e.into())
}
pub fn get_rsa_pubk_pkcs1_pem_file(pubk_path: &str) -> Result<RsaPublicKey, BoxError> {
    RsaPublicKey::read_pkcs1_pem_file(pubk_path).map_err(|e| e.into())
}
pub fn get_rsa_pubk_pkcs8_der(pubk: &str) -> Result<RsaPublicKey, BoxError> {
    RsaPublicKey::from_public_key_der(&BASE64_STANDARD.decode(pubk)?).map_err(|e| e.into())
}
pub fn get_rsa_pubk_pkcs8_pem_file(pubk_path: &str) -> Result<RsaPublicKey, BoxError> {
    RsaPublicKey::read_public_key_pem_file(pubk_path).map_err(|e| e.into())
}

pub fn get_rsa_prik_pkcs1_der(pubk: &str) -> Result<RsaPrivateKey, BoxError> {
    RsaPrivateKey::from_pkcs1_der(&BASE64_STANDARD.decode(pubk)?).map_err(|e| e.into())
}
pub fn get_rsa_prik_pkcs1_pem_file(pubk_path: &str) -> Result<RsaPrivateKey, BoxError> {
    RsaPrivateKey::read_pkcs1_pem_file(pubk_path).map_err(|e| e.into())
}
pub fn get_rsa_prik_pkcs8_der(pubk: &str) -> Result<RsaPrivateKey, BoxError> {
    RsaPrivateKey::from_pkcs8_der(&BASE64_STANDARD.decode(pubk)?).map_err(|e| e.into())
}
pub fn get_rsa_prik_pkcs8_pem_file(pubk_path: &str) -> Result<RsaPrivateKey, BoxError> {
    RsaPrivateKey::read_pkcs8_pem_file(pubk_path).map_err(|e| e.into())
}

//rsa 公钥加密
pub fn rsa_pubk_encrypt(pubk: &RsaPublicKey, target: &[u8]) -> Result<Vec<u8>, BoxError> {
    pubk.encrypt(&mut OsRng, Pkcs1v15Encrypt, target)
        .map_err(|e| e.into())
}

//rsa 私钥解密
pub fn rsa_prik_decrypt(prik: &RsaPrivateKey, target: &[u8]) -> Result<Vec<u8>, BoxError> {
    prik.decrypt(Pkcs1v15Encrypt, target).map_err(|e| e.into())
}

//rsa 私钥加签
pub fn rsa_sha256_prik_sign(prik: &RsaPrivateKey, target: &[u8]) -> Result<Vec<u8>, BoxError> {
    let binding = sha2::Sha256::digest(target);
    let target = binding.as_slice();
    prik.sign(Pkcs1v15Sign::new::<sha2::Sha256>(), target)
        .map_err(|e| e.into())
}

//rsa 公钥验签
pub fn rsa_sha256_pubk_verify(
    pubk: &RsaPublicKey,
    target: &[u8],
    sign: &[u8],
) -> Result<(), BoxError> {
    let binding = sha2::Sha256::digest(target);
    let target = binding.as_slice();
    pubk.verify(Pkcs1v15Sign::new::<sha2::Sha256>(), target, sign)
        .map_err(|e| e.into())
}

pub fn build_check_str(value: &Value, sign_str: &mut String) {
    if value.is_null() {
        return;
    }
    if let serde_json::Value::Object(ob) = value {
        let mut list: Vec<(&String, &Value)> = ob.iter().collect();
        list.sort_by(|e1, e2| e1.0.cmp(e2.0));
        list.into_iter().filter(|e| !e.1.is_null()).for_each(|e| {
            sign_str.push_str(e.0);
            sign_str.push('=');
            build_check_str(e.1, sign_str)
        });
    } else if let serde_json::Value::Array(array) = value {
        for item in array {
            build_check_str(item, sign_str);
        }
    } else {
        let value_str = value.to_string();
        let value_str = if value_str.starts_with('"') {
            &value_str[1..value_str.len() - 1]
        } else {
            value_str.as_str()
        };
        sign_str.push_str(value_str);
        sign_str.push('&');
    }
}

pub fn build_check_str_v2(
    path: &str,
    version: &str,
    timestamp: &str,
    token: &str,
    data: &str,
) -> String {
    format!("{}\n{}\n{}\n{}\n{}", path, version, timestamp, token, data)
}

pub fn build_rsa_pubk_to_base64(modulus: &[u8], exponent: &[u8]) -> Result<String, BoxError> {
    let public_key = RsaPublicKey::new(
        BigUint::from_bytes_be(modulus),
        BigUint::from_bytes_be(exponent),
    )?;
    Ok(BASE64_STANDARD.encode(public_key.to_pkcs1_der()?.as_bytes()))
}

#[test]
fn test() {
    //生成待加签字符串
    let json_str = "{\"user_id\":159,\"timestamp\":1722237507,\"data\":{\"order_title\":\"test transfer\",\"trade_id\":\"test12345w1\",\"trans_amount\":500,\"target_account\":{\"identity_type\":\"AliPayLogonId\",\"identity\":\"18698630396\",\"username\":\"王思诚\"},\"remark\":\"testremark\"}}";
    let json_value: Value = serde_json::from_str(json_str).unwrap();
    let mut sign_str = String::new();
    build_check_str(&json_value, &mut sign_str);
    println!("sign_str : {}", sign_str);
    let start_time = crate::utils::date_util::get_now_date_time_as_millis();
    let (prik, pubk) = build_rsa_pair();
    println!("prik : {}", prik);
    println!("pubk : {}", pubk);
    let pubk = get_rsa_pubk_pkcs1_der(&pubk).unwrap();
    let prik = get_rsa_prik_pkcs1_der(&prik).unwrap();
    //公钥加密
    let en = rsa_pubk_encrypt(&pubk, "大会速度哈U盾花洒电弧电话手打".as_bytes()).unwrap();
    println!("加密后数据 : {:?}", String::from_utf8(en.clone()));
    //私钥解密
    let de = rsa_prik_decrypt(&prik, &en).unwrap();
    println!("解密后数据 : {:?}", String::from_utf8(de));
    //私钥加签
    let sign = rsa_sha256_prik_sign(&prik, "dadsdadasd".as_bytes()).unwrap();
    //公钥验签
    println!(
        "验签结果 : {:?}",
        rsa_sha256_pubk_verify(&pubk, "dadsdadasd".as_bytes(), &sign)
    );
    println!(
        "处理完毕 : {}",
        crate::utils::date_util::get_now_date_time_as_millis() - start_time
    );
}
