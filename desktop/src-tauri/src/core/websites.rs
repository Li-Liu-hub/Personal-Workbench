use crate::{
    error::{DeskError, Result},
    model::Website,
};

pub fn prepare(mut website: Website) -> Result<Website> {
    let input = website.url.trim();
    let candidate = if input.contains("://") {
        input.to_owned()
    } else {
        format!("https://{input}")
    };
    let parsed = url::Url::parse(&candidate)
        .map_err(|_| DeskError::InvalidPath("请输入可用的网站地址".into()))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || input.is_empty()
    {
        return Err(DeskError::InvalidPath(
            "网站地址需要使用 http:// 或 https://".into(),
        ));
    }
    website.title = website.title.trim().to_owned();
    if website.title.is_empty() {
        website.title = parsed.host_str().unwrap_or_default().to_owned();
    }
    website.url = parsed.to_string();
    website.category = website.category.trim().to_owned();
    Ok(website)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn website_addresses_accept_http_hosts_and_reject_other_schemes_or_missing_hosts() {
        for input in [
            "",
            " ",
            "file:///C:/data",
            "ftp://example.test",
            "https://",
            "https://[invalid]",
        ] {
            assert!(
                prepare(Website {
                    url: input.into(),
                    ..Default::default()
                })
                .is_err(),
                "{input}"
            );
        }
        for input in [
            "example.test/a?q=1",
            "http://localhost:8080/",
            "https://例子.测试/",
        ] {
            let website = prepare(Website {
                url: input.into(),
                title: " title ".into(),
                category: " tag ".into(),
                ..Default::default()
            })
            .unwrap();
            assert_eq!(website.title, "title");
            assert_eq!(website.category, "tag");
            assert!(url::Url::parse(&website.url).is_ok());
        }
    }
}
