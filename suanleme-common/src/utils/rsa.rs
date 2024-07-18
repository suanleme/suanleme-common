use base64::{prelude::BASE64_STANDARD, Engine};
use rand::rngs::OsRng;
use rsa::{
    pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey, EncodeRsaPrivateKey, EncodeRsaPublicKey},
    Pkcs1v15Encrypt, Pkcs1v15Sign, RsaPrivateKey, RsaPublicKey,
};
use serde_json::Value;

use crate::error::BoxError;

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

//rsa 公钥加密
pub fn rsa_pubk_encrypt(pubk: &str, target: &[u8]) -> Result<Vec<u8>, BoxError> {
    let pubk = RsaPublicKey::from_pkcs1_der(&BASE64_STANDARD.decode(pubk)?)?;
    pubk.encrypt(&mut OsRng, Pkcs1v15Encrypt, target)
        .map_err(|e| e.into())
}

//rsa 私钥解密
pub fn rsa_prik_decrypt(prik: &str, target: &[u8]) -> Result<Vec<u8>, BoxError> {
    let prik = RsaPrivateKey::from_pkcs1_der(&BASE64_STANDARD.decode(prik)?)?;
    prik.decrypt(Pkcs1v15Encrypt, target).map_err(|e| e.into())
}

//rsa 私钥加签
pub fn rsa_prik_sign(prik: &str, target: &[u8]) -> Result<Vec<u8>, BoxError> {
    let prik = RsaPrivateKey::from_pkcs1_der(&BASE64_STANDARD.decode(prik)?)?;
    prik.sign(Pkcs1v15Sign::new_unprefixed(), target)
        .map_err(|e| e.into())
}

//rsa 公钥验签
pub fn rsa_pubk_verify(pubk: &str, target: &[u8], sign: &[u8]) -> Result<(), BoxError> {
    let pubk = RsaPublicKey::from_pkcs1_der(&BASE64_STANDARD.decode(pubk)?)?;
    pubk.verify(Pkcs1v15Sign::new_unprefixed(), target, sign)
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
            sign_str.push(':');
            build_check_str(e.1, sign_str)
        });
    } else {
        sign_str.push_str(&value.to_string());
        sign_str.push(':');
    }
}

#[test]
fn test() {
    let start_time = crate::utils::date_util::get_now_date_time_as_millis();
    let (prik, pubk) = build_rsa_pair();
    println!("prik : {}", prik);
    println!("pubk : {}", pubk);
    //公钥加密
    let en = rsa_pubk_encrypt(&pubk, "大会速度哈U盾花洒电弧电话手打".as_bytes()).unwrap();
    println!("加密后数据 : {:?}", String::from_utf8(en.clone()));
    //私钥解密
    let de = rsa_prik_decrypt(&prik, &en).unwrap();
    println!("解密后数据 : {:?}", String::from_utf8(de));
    //私钥加签
    let sign = rsa_prik_sign(&prik, "dadsdadasd".as_bytes()).unwrap();
    //公钥验签
    println!(
        "验签结果 : {:?}",
        rsa_pubk_verify(&pubk, "dadsdadasd".as_bytes(), &sign)
    );
    println!(
        "处理完毕 : {}",
        crate::utils::date_util::get_now_date_time_as_millis() - start_time
    );
}
