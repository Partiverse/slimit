//! SlimIt AI 解释层。
//!
//! 红线（SPEC §5，不可协商）：AI 输出只进 UI 提示层，永不写入规则匹配
//! 结果、永不影响执行器授权。执行授权只来自规则库（slimit-rules）。

use serde::{Deserialize, Serialize};

/// 解释请求输入：路径 + 上下文。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplanationRequest {
    pub path: String,
    /// 真实占用字节。
    pub actual_bytes: u64,
    /// 表观大小字节。
    pub apparent_bytes: u64,
    /// 所属应用 bundle id（可从 .app/Info.plist 反查；未知为 None）。
    pub owner_bundle: Option<String>,
    /// 最近命中的规则 id（有规则命中时 AI 仅补充语气，不另判风险）。
    pub nearest_rule_hits: Vec<String>,
}

/// 解释输出。`suggested_risk` 仅作 UI 提示；执行器永不读取本结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Explanation {
    pub what: String,
    pub producer: String,
    pub consequence: String,
    pub suggested_risk: RiskHint,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RiskHint {
    Green,
    Yellow,
    Red,
}

/// 解释器 trait。模型版（GGUF/candle，W6）与云端版实现同一接口。
pub trait Explainer: Send + Sync {
    fn explain(&self, req: &ExplanationRequest) -> Result<Explanation, String>;
}

/// 启发式降级实现：无模型。基于路径分词 + 稀疏比 + owner bundle 的模板解释。
/// 这保证产品在"AI 模型缺失"时依然交付完整语义解释（规则库兜底为主）。
pub struct HeuristicExplainer;

impl Explainer for HeuristicExplainer {
    fn explain(&self, req: &ExplanationRequest) -> Result<Explanation, String> {
        let c = classify(&req.path, req.apparent_bytes, req.actual_bytes);
        Ok(Explanation {
            what: c.description,
            producer: c
                .producer
                .or_else(|| req.owner_bundle.as_ref().map(|b| format!("应用 {b}")))
                .unwrap_or_else(|| "未知（路径未命中规则库，建议保持谨慎）".into()),
            consequence: c.consequence,
            suggested_risk: c.risk_hint,
            confidence: c.confidence,
        })
    }
}

struct Classification {
    description: String,
    producer: Option<String>,
    consequence: String,
    risk_hint: RiskHint,
    confidence: f32,
}

fn classify(path: &str, apparent: u64, actual: u64) -> Classification {
    let p = path.to_lowercase();
    let sparse_note = || {
        if apparent > actual * 2 && apparent > 1 << 30 {
            format!(
                "（稀疏文件：显示 {:.1} GB，实际占用 {:.1} GB）",
                apparent as f64 / 1073741824.0,
                actual as f64 / 1073741824.0
            )
        } else {
            String::new()
        }
    };

    // 模式表：顺序即优先级。新场景在此加行（规则库命中优先于本表）。
    // 列：路径子串、是什么、谁产生（可空）、删了会怎样、风险提示、置信度。
    type Row = (
        &'static str,
        &'static str,
        Option<&'static str>,
        &'static str,
        RiskHint,
        f32,
    );
    const TABLE: &[Row] = &[
        (
            "deriveddata",
            "Xcode 增量构建产物与索引",
            Some("Xcode"),
            "下次构建全量重编译，无数据丢失",
            RiskHint::Green,
            0.9,
        ),
        (
            "node_modules",
            "npm/yarn 依赖目录",
            Some("npm 等 JS 包管理器"),
            "npm install 可完整重建",
            RiskHint::Green,
            0.9,
        ),
        (
            ".cache",
            "应用/工具缓存",
            None,
            "应用按需重建，短期性能略降",
            RiskHint::Green,
            0.7,
        ),
        (
            "caches",
            "应用/工具缓存",
            None,
            "应用按需重建，短期性能略降",
            RiskHint::Green,
            0.7,
        ),
        (
            "logs",
            "应用日志",
            None,
            "旧日志删除一般安全，正在写入的除外",
            RiskHint::Green,
            0.6,
        ),
        (
            "backups",
            "设备/应用备份",
            None,
            "删除后对应历史备份不可恢复",
            RiskHint::Yellow,
            0.6,
        ),
        (
            "mobilesync",
            "iPhone/iPad Finder 整机备份",
            Some("Finder"),
            "对应设备的备份永久丢失",
            RiskHint::Yellow,
            0.8,
        ),
        (
            "docker.raw",
            "Docker Desktop 虚拟磁盘",
            Some("Docker Desktop"),
            "所有镜像/容器/卷数据，绝不可直接删文件",
            RiskHint::Red,
            0.85,
        ),
        (
            "ext4.vhdx",
            "WSL 发行版虚拟磁盘",
            Some("WSL"),
            "整个 Linux 发行版文件系统，绝不可直接删",
            RiskHint::Red,
            0.85,
        ),
        (
            "archives",
            "应用归档（可能含符号化依据）",
            None,
            "可能不可逆丢失发布记录",
            RiskHint::Yellow,
            0.5,
        ),
        (
            "wechat_files",
            "微信聊天数据目录",
            Some("微信"),
            "聊天记录与文件永久丢失",
            RiskHint::Red,
            0.9,
        ),
        (
            "tencent files",
            "QQ 聊天数据目录",
            Some("QQ"),
            "聊天记录与文件永久丢失",
            RiskHint::Red,
            0.9,
        ),
    ];

    for (needle, desc, producer, cons, risk, conf) in TABLE {
        if p.contains(needle) {
            return Classification {
                description: format!("{desc}{}", sparse_note()),
                producer: producer.map(|s| s.to_string()),
                consequence: (*cons).to_string(),
                risk_hint: *risk,
                confidence: *conf,
            };
        }
    }

    Classification {
        description: format!(
            "未识别目录{}，包含用户数据或应用数据的可能性未知",
            sparse_note()
        ),
        producer: None,
        consequence: "未知。未命中规则库，默认按最保守处理：不做任何自动删除".into(),
        risk_hint: RiskHint::Red,
        confidence: 0.3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(path: &str) -> ExplanationRequest {
        ExplanationRequest {
            path: path.into(),
            actual_bytes: 1 << 30,
            apparent_bytes: 1 << 30,
            owner_bundle: None,
            nearest_rule_hits: vec![],
        }
    }

    #[test]
    fn known_paths_classified() {
        let e = HeuristicExplainer
            .explain(&req("/Users/x/Library/Developer/Xcode/DerivedData/app-abc"))
            .unwrap();
        assert_eq!(e.suggested_risk, RiskHint::Green);
        assert_eq!(e.producer, "Xcode");
    }

    #[test]
    fn dangerous_paths_red() {
        for p in [
            "/Users/x/AppData/Local/Docker/wsl/data/ext4.vhdx",
            "/Users/x/Documents/WeChat Files",
        ] {
            let e = HeuristicExplainer.explain(&req(p)).unwrap();
            assert_eq!(e.suggested_risk, RiskHint::Red, "path: {p}");
        }
    }

    #[test]
    fn unknown_paths_default_red_low_confidence() {
        let e = HeuristicExplainer
            .explain(&req("/Users/x/神秘目录"))
            .unwrap();
        assert_eq!(e.suggested_risk, RiskHint::Red);
        assert!(e.confidence < 0.5);
    }

    #[test]
    fn sparse_note_appended() {
        let mut r = req("/vm/Docker.raw");
        r.apparent_bytes = 500 << 30;
        r.actual_bytes = 40 << 30;
        let e = HeuristicExplainer.explain(&r).unwrap();
        assert!(e.what.contains("稀疏文件"), "what: {}", e.what);
    }
}
