use serde::Deserialize;

pub fn parse_yaml(yaml_str: &str) -> anyhow::Result<Vec<SpliceRule>> {
    let config: ConfigFile = serde_yaml::from_str(yaml_str)?;
    
    // i'm only able to parse this config version!
    assert_eq!(config.version, 1);
    Ok(config.to_splice_rules())

}

/// --- YAML config structures ---
#[derive(Debug, Deserialize)]
pub struct ConfigFile {
    pub version: u32,
    pub rules: Vec<YamlRule>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum YamlRule {
    Inject {
        match_on: YamlMatch,
        inject: YamlInject,
    },
    Between {
        match_on: YamlMatch,
        between: YamlBetweenComponent,
        middlewares: Vec<String>,
    },
}

#[derive(Debug, Deserialize)]
pub struct YamlMatch {
    pub interface: String,
    pub provider_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct YamlInject {
    pub middlewares: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct YamlBetweenComponent {
    pub inner: String,
    pub outer: String,
}

/// --- Normalized rule type for Rust usage ---
#[derive(Debug)]
pub enum SpliceRule {
    Inject {
        interface: String,
        provider_name: Option<String>,
        middlewares: Vec<String>,
    },
    Between {
        interface: String,
        inner: String,
        outer: String,
        middlewares: Vec<String>,
    },
}

impl ConfigFile {
    /// Convert YAML parsed rules into normalized [SpliceRule]
    pub fn to_splice_rules(&self) -> Vec<SpliceRule> {
        self.rules.iter().map(|r| match r {
            YamlRule::Inject { match_on, inject } => SpliceRule::Inject {
                interface: match_on.interface.clone(),
                provider_name: match_on.provider_name.clone(),
                middlewares: inject.middlewares.clone(),
            },
            YamlRule::Between { match_on, between, middlewares } => SpliceRule::Between {
                interface: match_on.interface.clone(),
                inner: between.inner.clone(),
                outer: between.outer.clone(),
                middlewares: middlewares.clone(),
            },
        }).collect()
    }
}