use crate::{
    ApprovalDatabase, ApprovalScope, ProcessLineage, Result, SecureConfig, SecureError,
    GRANTED_BY_INTERACTIVE,
};
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecureVarSource {
    AutoDetected { pattern: String },
    Explicit,
    Config { file: PathBuf },
}

#[derive(Debug, Clone)]
pub struct SecureVariable {
    pub name: String,
    pub value: String,
    pub source: SecureVarSource,
    pub created_at: DateTime<Utc>,
}

impl SecureVariable {
    pub fn new(name: String, value: String, source: SecureVarSource) -> Self {
        Self {
            name,
            value,
            source,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessResult {
    Allowed(String),
    Denied,
    Pending(u64),
    NotSecure(String),
}

pub struct SecureRegistry {
    config: SecureConfig,
    approvals: ApprovalDatabase,
    lineage: ProcessLineage,
    secure_vars: Arc<RwLock<HashMap<String, SecureVariable>>>,
    pending_requests: Arc<RwLock<HashMap<u64, PendingAccess>>>,
    request_counter: Arc<RwLock<u64>>,
}

struct PendingAccess {
    variable: String,
    script_path: PathBuf,
    script_hash: String,
}

impl SecureRegistry {
    pub fn new(config: SecureConfig, approvals: ApprovalDatabase) -> Self {
        Self {
            config,
            approvals,
            lineage: ProcessLineage::new(),
            secure_vars: Arc::new(RwLock::new(HashMap::new())),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            request_counter: Arc::new(RwLock::new(0)),
        }
    }

    pub fn with_defaults() -> Result<Self> {
        let config = SecureConfig::load_or_default()?;
        let approvals = ApprovalDatabase::open_default()?;
        Ok(Self::new(config, approvals))
    }

    pub fn is_secure(&self, name: &str) -> bool {
        if self.secure_vars.read().contains_key(name) {
            return true;
        }
        self.config.is_secure(name)
    }

    pub fn mark_secure(&self, name: &str, value: &str, source: SecureVarSource) {
        let var = SecureVariable::new(name.to_string(), value.to_string(), source);
        self.secure_vars.write().insert(name.to_string(), var);
    }

    pub fn unmark_secure(&self, name: &str) {
        self.secure_vars.write().remove(name);
    }

    pub fn get_secure_value(&self, name: &str) -> Option<String> {
        self.secure_vars.read().get(name).map(|v| v.value.clone())
    }

    pub fn set_secure_value(&self, name: &str, value: &str) {
        let mut vars = self.secure_vars.write();
        if let Some(var) = vars.get_mut(name) {
            var.value = value.to_string();
        } else {
            let source = if let Some(pattern) = self.config.matching_pattern(name) {
                SecureVarSource::AutoDetected { pattern }
            } else {
                SecureVarSource::Explicit
            };
            vars.insert(
                name.to_string(),
                SecureVariable::new(name.to_string(), value.to_string(), source),
            );
        }
    }

    pub fn list_secure_vars(&self) -> Vec<(String, SecureVarSource, bool)> {
        let vars = self.secure_vars.read();
        vars.iter()
            .map(|(name, var)| (name.clone(), var.source.clone(), !var.value.is_empty()))
            .collect()
    }

    pub fn check_access(
        &self,
        variable: &str,
        requesting_script: Option<&Path>,
        process_id: u32,
    ) -> AccessResult {
        if !self.is_secure(variable) {
            if let Some(value) = self.get_secure_value(variable) {
                return AccessResult::NotSecure(value);
            }
            return AccessResult::NotSecure(String::new());
        }

        let value = match self.get_secure_value(variable) {
            Some(v) => v,
            None => return AccessResult::Denied,
        };

        if self.lineage.has_inherited_approval(process_id, variable) {
            return AccessResult::Allowed(value);
        }

        let Some(script_path) = requesting_script else {
            return AccessResult::Denied;
        };

        let script_hash = match compute_script_hash(script_path) {
            Ok(h) => h,
            Err(_) => return AccessResult::Denied,
        };

        match self
            .approvals
            .check_approval(script_path, &script_hash, variable)
        {
            Ok(true) => {
                self.lineage.grant_approval(process_id, variable);
                AccessResult::Allowed(value)
            }
            Ok(false) => {
                let request_id = self.create_pending_request(variable, script_path, &script_hash);
                AccessResult::Pending(request_id)
            }
            Err(_) => AccessResult::Denied,
        }
    }

    pub fn handle_approval_response(
        &self,
        request_id: u64,
        decision: ApprovalDecision,
    ) -> Result<Option<String>> {
        let pending = {
            let mut requests = self.pending_requests.write();
            requests.remove(&request_id)
        };

        let Some(pending) = pending else {
            return Err(SecureError::NotFound(format!("Request {}", request_id)));
        };

        match decision {
            ApprovalDecision::AllowOnce => {
                let value = self.get_secure_value(&pending.variable);
                Ok(value)
            }
            ApprovalDecision::Remember => {
                self.approvals.save_approval(
                    &pending.script_path,
                    &pending.script_hash,
                    &pending.variable,
                    GRANTED_BY_INTERACTIVE,
                    ApprovalScope::Global,
                )?;
                let value = self.get_secure_value(&pending.variable);
                Ok(value)
            }
            ApprovalDecision::Deny => Ok(None),
        }
    }

    pub fn register_child(&self, parent_pid: u32, child_pid: u32) {
        self.lineage.register_child(parent_pid, child_pid);
    }

    pub fn revoke_approval(&self, variable: &str, script_path: Option<&Path>) -> Result<()> {
        if let Some(path) = script_path {
            self.approvals.revoke_approval(path, variable)
        } else {
            self.approvals.revoke_all_approvals(variable)
        }
    }

    pub fn list_approvals(&self, variable: &str) -> Result<Vec<crate::SecureApproval>> {
        self.approvals.list_approvals(variable)
    }

    /// Redact known secret values from a string.
    ///
    /// Replaces any substring matching a stored credential value with `***`.
    /// Secret values never leave this crate — only the sanitized result is returned.
    pub fn redact(&self, input: &str) -> String {
        let vars = self.secure_vars.read();
        let mut result = input.to_string();
        for var in vars.values() {
            if !var.value.is_empty() {
                result = result.replace(&var.value, "***");
            }
        }
        result
    }

    fn create_pending_request(&self, variable: &str, script_path: &Path, script_hash: &str) -> u64 {
        let mut counter = self.request_counter.write();
        *counter += 1;
        let request_id = *counter;

        let pending = PendingAccess {
            variable: variable.to_string(),
            script_path: script_path.to_path_buf(),
            script_hash: script_hash.to_string(),
        };

        self.pending_requests.write().insert(request_id, pending);
        request_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    AllowOnce,
    Remember,
    Deny,
}

pub fn compute_script_hash(path: &Path) -> Result<String> {
    let content = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    let result = hasher.finalize();
    Ok(format!("sha256:{}", hex::encode(result)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_compute_script_hash() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "#!/bin/bash\necho hello").unwrap();

        let hash = compute_script_hash(file.path()).unwrap();
        assert!(hash.starts_with("sha256:"));
        assert_eq!(hash.len(), 7 + 64);
    }

    #[test]
    fn test_secure_variable_creation() {
        let var = SecureVariable::new(
            "AWS_SECRET_KEY".to_string(),
            "secret123".to_string(),
            SecureVarSource::Explicit,
        );

        assert_eq!(var.name, "AWS_SECRET_KEY");
        assert_eq!(var.value, "secret123");
        assert_eq!(var.source, SecureVarSource::Explicit);
    }

    #[test]
    fn test_redact_replaces_known_secrets() {
        let registry = SecureRegistry::with_defaults().unwrap();
        registry.set_secure_value("API_KEY", "sk-abc123xyz");

        let input = "curl -H 'Authorization: Bearer sk-abc123xyz' https://api.example.com";
        let redacted = registry.redact(input);

        assert!(!redacted.contains("sk-abc123xyz"));
        assert!(redacted.contains("***"));
        assert!(redacted.contains("https://api.example.com"));
    }

    #[test]
    fn test_redact_no_secrets_returns_unchanged() {
        let registry = SecureRegistry::with_defaults().unwrap();
        let input = "echo hello world";
        let redacted = registry.redact(input);
        assert_eq!(redacted, input);
    }

    #[test]
    fn test_access_result_variants() {
        let allowed = AccessResult::Allowed("value".to_string());
        assert_eq!(allowed, AccessResult::Allowed("value".to_string()));

        let denied = AccessResult::Denied;
        assert_eq!(denied, AccessResult::Denied);

        let pending = AccessResult::Pending(42);
        assert_eq!(pending, AccessResult::Pending(42));
    }
}
