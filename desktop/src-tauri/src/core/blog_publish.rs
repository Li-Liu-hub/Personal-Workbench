use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Serialize;

const TENANT_ID: &str = "0000000000000000";
const MAX_TITLE_CHARS: usize = 200;
const MAX_CONTENT_BYTES: usize = 200_000;
pub const MAX_TAGS: usize = 8;

#[derive(Clone, Debug, Serialize)]
pub struct BlogPostPayload {
    pub page_id: String,
    pub post_id: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub tenant_id: String,
    pub published_at: i64,
    pub published_date: String,
}

/// Blog publishing limits shared by the desktop command layer and the SSH
/// publisher, so the same rules and error texts are used in both places.
pub fn validate_post(title: &str, body: &str, tags: &[String]) -> crate::error::Result<()> {
    use crate::error::DeskError;
    if title.trim().is_empty() || title.chars().count() > MAX_TITLE_CHARS {
        return Err(DeskError::InvalidInput(
            "博客标题不能为空，且不能超过 200 个字符".into(),
        ));
    }
    if body.len() > MAX_CONTENT_BYTES {
        return Err(DeskError::InvalidInput(
            "博客正文超过 200 KB，请拆分文档后再发布".into(),
        ));
    }
    if tags.len() > MAX_TAGS || tags.iter().any(|tag| tag.trim().chars().count() > 50) {
        return Err(DeskError::InvalidInput(
            "博客标签最多 8 个，每个标签不能超过 50 个字符".into(),
        ));
    }
    Ok(())
}

pub fn default_ssh_key() -> Option<PathBuf> {
    let path = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)?
        .join(".ssh")
        .join("server_management_ed25519");
    path.is_file().then_some(path)
}

pub fn publish_over_ssh(key_path: &Path, payload: &BlogPostPayload) -> Result<String, String> {
    validate_post(&payload.title, &payload.content, &payload.tags).map_err(|e| e.to_string())?;
    if payload.tenant_id != TENANT_ID
        || payload.post_id.len() != 16
        || !payload.post_id.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("博客发布信息无效。".into());
    }

    let key_path = key_path
        .canonicalize()
        .map_err(|error| format!("无法读取 SSH 私钥：{error}"))?;
    if !key_path.is_file() {
        return Err("所选 SSH 私钥文件不存在。".into());
    }
    let payload = serde_json::to_vec(payload).map_err(|error| error.to_string())?;
    let script = STANDARD.encode(include_str!("../../../../assets/blog/ssh_publish.py"));
    let remote_command = format!("python3 -c \"import base64;exec(base64.b64decode('{script}'))\"");
    let ssh = std::env::var_os("WINDIR")
        .map(|root| PathBuf::from(root).join("System32/OpenSSH/ssh.exe"))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("ssh"));

    let mut child = Command::new(ssh)
        .args([
            "-T",
            "-o",
            "BatchMode=yes",
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "ConnectTimeout=12",
            "-o",
            "ServerAliveInterval=10",
            "-o",
            "ServerAliveCountMax=3",
            "-i",
        ])
        .arg(key_path)
        .args(["-p", "22", "ubuntu@49.234.49.192", &remote_command])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("启动 SSH 失败：{error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "无法连接 SSH 输入流。".to_owned())?
        .write_all(&payload)
        .map_err(|error| format!("发送文章到服务器失败：{error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("等待 SSH 发布结果失败：{error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.trim();
        return Err(if detail.is_empty() {
            "SSH 发布失败，请检查 SSH 密钥、服务器连接和服务器数据库状态。".into()
        } else {
            format!(
                "SSH 发布失败：{}",
                detail.chars().take(600).collect::<String>()
            )
        });
    }
    let response = String::from_utf8_lossy(&output.stdout);
    let expected = format!("PUBLISHED {}", payload_post_id(&payload)?);
    if response.trim() != expected {
        return Err("服务器已返回，但未能确认文章发布结果。再次点击会更新同一篇文章。".into());
    }
    Ok(expected)
}

fn payload_post_id(json: &[u8]) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct PostId {
        post_id: String,
    }
    serde_json::from_slice::<PostId>(json)
        .map(|payload| payload.post_id)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(count: usize) -> Vec<String> {
        (0..count).map(|index| format!("标签{index}")).collect()
    }

    #[test]
    fn post_validation_accepts_publishable_content() {
        validate_post("标题", "正文", &tags(2)).unwrap();
        validate_post(&"字".repeat(MAX_TITLE_CHARS), "正文", &tags(MAX_TAGS)).unwrap();
    }

    #[test]
    fn post_validation_rejects_oversized_title_body_and_tags() {
        assert_eq!(
            validate_post("  ", "正文", &[]).unwrap_err().to_string(),
            "博客标题不能为空，且不能超过 200 个字符"
        );
        assert_eq!(
            validate_post(&"字".repeat(MAX_TITLE_CHARS + 1), "正文", &[])
                .unwrap_err()
                .to_string(),
            "博客标题不能为空，且不能超过 200 个字符"
        );
        assert_eq!(
            validate_post("标题", &"文".repeat(MAX_CONTENT_BYTES + 1), &[])
                .unwrap_err()
                .to_string(),
            "博客正文超过 200 KB，请拆分文档后再发布"
        );
        assert_eq!(
            validate_post("标题", "正文", &tags(MAX_TAGS + 1))
                .unwrap_err()
                .to_string(),
            "博客标签最多 8 个，每个标签不能超过 50 个字符"
        );
        let mut long_tag = tags(1);
        long_tag[0] = "标".repeat(51);
        assert_eq!(
            validate_post("标题", "正文", &long_tag)
                .unwrap_err()
                .to_string(),
            "博客标签最多 8 个，每个标签不能超过 50 个字符"
        );
    }
}
