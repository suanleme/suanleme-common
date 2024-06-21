use serde_json::json;
use serde_yaml;

use crate::error::BoxError;

pub fn get_yaml_by_context<'a, T: serde::Deserialize<'a>>(
    yaml_context: &str,
) -> Result<T, BoxError> {
    // 解析 TOML 文件内容
    let parsed_toml = serde_yaml::from_str(yaml_context)?;
    let json = json!(parsed_toml);
    Ok(T::deserialize(json).map_err(|e| format!("toml to json error {:?}", e))?)
}
