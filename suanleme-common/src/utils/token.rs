use crate::{log::get_uuid, utils::date_util::get_now_date_time};

pub fn get_token() -> String {
    format!("{}-{}", get_uuid(), get_now_date_time())
}

#[test]
fn test() {
    println!("{:?}", get_token());
}
