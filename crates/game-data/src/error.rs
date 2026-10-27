//! 数据加载错误。校验错误一律带文件名、条目和可读建议。

use std::path::PathBuf;
use thiserror::Error;

/// 内存中的一份命名数据源，测试用内联 RON，不读磁盘。
#[derive(Debug, Clone)]
pub struct NamedSource {
    /// 逻辑文件名，例如 `monsters.ron`。
    pub name: String,
    /// RON 文本。
    pub text: String,
}

impl NamedSource {
    /// 构造一份命名源。
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            text: text.into(),
        }
    }
}

/// 读表或校验失败。
#[derive(Debug, Error)]
pub enum DataError {
    /// 磁盘读取失败。
    #[error("{path}: 无法读取（{source}）")]
    Io {
        /// 文件路径。
        path: PathBuf,
        /// 底层 IO 错误。
        #[source]
        source: std::io::Error,
    },
    /// RON 语法或类型不匹配。
    #[error("{file}: RON 解析失败（{message}）")]
    Parse {
        /// 逻辑文件名。
        file: String,
        /// ron 报错文本。
        message: String,
    },
    /// 语义校验失败。
    #[error("{file}: '{entry}' {message}{suggestion}")]
    Validation {
        /// 逻辑文件名。
        file: String,
        /// 条目 slug；整表级错误用 `*`。
        entry: String,
        /// 问题描述。
        message: String,
        /// 已带括号的修正建议，没有则为空串。
        suggestion: String,
    },
}

impl DataError {
    pub(crate) fn validation(
        file: &str,
        entry: &str,
        message: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        let suggestion = suggestion.into();
        let suggestion = if suggestion.is_empty() {
            String::new()
        } else {
            format!("（{suggestion}）")
        };
        Self::Validation {
            file: file.to_string(),
            entry: entry.to_string(),
            message: message.into(),
            suggestion,
        }
    }
}
