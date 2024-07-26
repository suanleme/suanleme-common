use rand::{distributions::Alphanumeric, thread_rng, Rng};

use crate::{log::get_uuid, utils::date_util::get_now_date_time};

pub fn get_token() -> String {
    format!("{}-{}", get_uuid(), get_now_date_time())
}

pub fn get_rng_str(len: usize) -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

#[test]
fn test() {
    println!("{:?}", get_token());
    println!("{:?}", get_rng_str(22));
}
