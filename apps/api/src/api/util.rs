use crate::{infrastructure::WebDb, infrastructure::settings::get_setting};
use anyhow::{Result, anyhow};
use lettre::message::header;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};

pub async fn send_email(
    db: &WebDb,
    to_list: &[&str],         // 收件人列表
    cc_list: Option<&[&str]>, // 抄送列表，可选
    subject: &str,            // 可选主题
    body: &str,
) -> Result<()> {
    let smtp_uri = get_setting(db.get_ref(), "SMTP_URI")
        .await
        .map_err(|e| anyhow!("Failed to get SMTP_URI: {}", e))?;
    // smtp.163.com:user@163.com:password
    // 解析成三部分
    let parts: Vec<&str> = smtp_uri.split(':').collect();
    if parts.len() != 3 {
        return Err(anyhow!(
            "Invalid SMTP_URI format, expected server:user:pass"
        ));
    }
    let smtp_server = parts[0];
    let smtp_user = parts[1];
    let smtp_pass = parts[2];

    // building message
    let mut builder = Message::builder().from(smtp_user.parse()?);

    if to_list.is_empty() {
        return Err(anyhow!("Recipient list cannot be empty"));
    }

    for recipient in to_list {
        builder = builder.to(recipient.parse()?);
    }

    // 添加抄送列表
    if let Some(cc) = cc_list {
        for recipient in cc {
            builder = builder.cc(recipient.parse()?);
        }
    }

    // 设置主题和正文
    let email = builder
        .subject(subject)
        .header(header::ContentType::TEXT_HTML)
        .body(body.to_string())?;

    // SMTP 传输器
    let creds = Credentials::new(smtp_user.to_string(), smtp_pass.to_string());
    let mailer = SmtpTransport::relay(smtp_server)?
        .credentials(creds)
        .build();

    // 发送邮件
    mailer
        .send(&email)
        .map_err(|e| anyhow!("Failed to send email: {}", e))?;

    Ok(())
}

pub fn none_if_empty(s: Option<String>) -> Option<String> {
    match s {
        Some(x) if x.trim().is_empty() => None,
        other => other,
    }
}
