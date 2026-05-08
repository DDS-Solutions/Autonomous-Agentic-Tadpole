#[cfg(test)]
mod tests {
    use serde::Serialize;
    use std::collections::HashMap;

    #[derive(Serialize)]
    struct Outer {
        b: String,
        #[serde(flatten)]
        extra: Option<HashMap<String, serde_json::Value>>,
    }

    #[test]
    fn test_flatten_option() {
        let mut extra = HashMap::new();
        extra.insert("skills".to_string(), serde_json::json!(["foo", "bar"]));
        
        let o = Outer {
            b: "hello".to_string(),
            extra: Some(extra),
        };

        match serde_json::to_string(&o) {
            Ok(s) => println!("Success: {}", s),
            Err(e) => panic!("Error: {}", e),
        }
    }
}
