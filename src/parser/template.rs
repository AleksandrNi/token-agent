use anyhow::{Result, anyhow};
use std::collections::HashMap;
use regex::Regex;

/// Render multi-source template
/// template: "Bearer {{source.field}}"
/// ctx: { "source" => { "field" => "value" } }
pub fn render_template(
    template: &str,
    ctx: &HashMap<String, HashMap<String, String>>
) -> Result<String> {
    // Regex for {{source.field}} placeholders
    let re = Regex::new(r"\{\{\s*([a-zA-Z0-9_]+)\.([a-zA-Z0-9_]+)\s*\}\}")?;

    let result = re.replace_all(template, |caps: &regex::Captures| {
        let source = &caps[1];
        let field = &caps[2];

        match ctx.get(source).and_then(|m| m.get(field)) {
            Some(val) => val.clone(),
            None => String::new(), // empty string if missing; optional: fail instead
        }
    });

    // If any {{...}} placeholders remain unresolved, fail
    if re.is_match(&result) && result.contains("{{") {
        return Err(anyhow!("Template contains unresolved placeholders: {}", result));
    }

    Ok(result.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_template_basic() {
        let mut ctx = HashMap::new();
        let mut meta = HashMap::new();
        meta.insert("token".to_string(), "abc123".to_string());
        ctx.insert("my_vm_metadata".to_string(), meta);

        let tpl = "Bearer {{my_vm_metadata.token}}";
        let rendered = render_template(tpl, &ctx).unwrap();
        assert_eq!(rendered, "Bearer abc123");
    }

    #[test]
    fn test_render_template_multi_source() {
        let mut ctx = HashMap::new();
        let mut meta = HashMap::new();
        meta.insert("token".to_string(), "abc123".to_string());
        ctx.insert("my_vm_metadata".to_string(), meta);

        let mut client = HashMap::new();
        client.insert("token".to_string(), "xyz789".to_string());
        ctx.insert("client_creds_token".to_string(), client);

        let tpl = "Bearer {{my_vm_metadata.token}}-{{client_creds_token.token}}";
        let rendered = render_template(tpl, &ctx).unwrap();
        assert_eq!(rendered, "Bearer abc123-xyz789");
    }

    #[test]
    fn test_render_template_missing_field() {
        let ctx: HashMap<String, HashMap<String, String>> = HashMap::new();
        let tpl = "Bearer {{missing.token}}";
        let result = render_template(tpl, &ctx);
        assert!(result.is_err());
    }
}
