use super::store::Job;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const DAYS: i64 = 15;
pub const WINDOW_SECONDS: i64 = DAYS * 24 * 60 * 60;

/// Preserve meaningful symbols such as C++, C# and path separators.
/// Ignore layout whitespace, full-width ASCII variants and copy/paste zero-width marks.
pub fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '\u{200b}' | '\u{feff}'))
        .map(|c| {
            if ('\u{ff01}'..='\u{ff5e}').contains(&c) {
                char::from_u32(c as u32 - 0xfee0).unwrap()
            } else {
                c
            }
        })
        .flat_map(char::to_lowercase)
        .collect()
}
pub fn fingerprint(job: &Job) -> Option<String> {
    let fields = [
        normalize(&job.company),
        normalize(&job.title),
        normalize(&job.jd),
    ];
    if fields.iter().any(String::is_empty) {
        return None;
    }
    let encoded = serde_json::to_vec(&fields).expect("strings serialize");
    Some(format!("{:x}", Sha256::digest(encoded)))
}
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Eligibility {
    Eligible,
    CoolingDown { until: i64 },
    NeedsReview { attempt_id: i64 },
    PreferredSource { url: String, platform: String },
}
impl Eligibility {
    pub fn error(&self) -> Option<String> {
        match self {
            Self::Eligible => None,
            Self::CoolingDown { .. } => {
                Some("同一岗位距上次成功投递尚未超过15天，所有平台和账号共享此去重记录".into())
            }
            Self::NeedsReview { .. } => {
                Some("同一岗位存在发送中、部分发送、待核验记录或疑似跨平台同岗位，已阻止重复投递，请核验原记录".into())
            }
            Self::PreferredSource{url,platform}=>Some(format!("同一岗位优先保留{}，请使用优先来源投递：{url}",super::tracking::site_label(platform))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composite_identity_normalizes_layout_without_merging_distinct_jobs() {
        let mut a = Job {
            company: "ＡＣＭＥ 科技".into(),
            title: "Java 后端".into(),
            jd: "熟悉 Spring\n和 C++".into(),
            ..Default::default()
        };
        let mut b = a.clone();
        b.company = "acme科技".into();
        b.title = "java后端".into();
        b.jd = "熟悉Spring 和C++".into();
        assert_eq!(fingerprint(&a), fingerprint(&b));
        b.jd = "熟悉Spring和C#".into();
        assert_ne!(fingerprint(&a), fingerprint(&b));
        b = a.clone();
        b.title = "AI应用开发".into();
        assert_ne!(fingerprint(&a), fingerprint(&b));
        a.jd.clear();
        assert!(fingerprint(&a).is_none());
    }
}
