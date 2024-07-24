use std::time::{SystemTime, UNIX_EPOCH};

use chrono::Local;

pub fn get_now_date_time_as_millis() -> u128 {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    since_the_epoch.as_millis()
}

pub fn get_now_date_time() -> String {
    Local::now().format("%Y%m%d%H%M%S").to_string()
}

#[test]
fn test() {
    // 格式化时间
    println!("格式化后的本地时间: {}", get_now_date_time());
}
