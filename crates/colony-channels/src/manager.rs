//! Channel manager with registry and alias resolution

use crate::{
    Channel, ChannelError, ChannelId, ContextChannel, Result, STDCONTEXT, STDLLM, STDTOOL, STDTRACE,
};
use std::collections::HashMap;

/// Manages all channels with fd and alias resolution
pub struct ChannelManager {
    /// Channel registry (fd → channel)
    channels: HashMap<ChannelId, Box<dyn Channel>>,

    /// Alias registry (name → fd)
    aliases: HashMap<String, ChannelId>,
}

impl ChannelManager {
    /// Create channel manager with default channels
    pub fn new() -> Self {
        let mut manager = Self {
            channels: HashMap::new(),
            aliases: HashMap::new(),
        };

        // Register standard channels
        manager.register(Box::new(ContextChannel::new()));

        // TODO: Register other channels (Phase 2)
        // manager.register(Box::new(LlmChannel::new()));
        // manager.register(Box::new(ToolChannel::new()));
        // manager.register(Box::new(TraceChannel::new()));

        // Register aliases
        manager.register_alias("ctx", STDCONTEXT);
        manager.register_alias("context", STDCONTEXT);
        manager.register_alias("llm", STDLLM);
        manager.register_alias("tool", STDTOOL);
        manager.register_alias("trace", STDTRACE);

        manager
    }

    /// Get context channel (convenience method)
    pub fn context(&mut self) -> Option<&mut ContextChannel> {
        self.channels
            .get_mut(&STDCONTEXT)?
            .as_any_mut()
            .downcast_mut::<ContextChannel>()
    }

    /// Register a channel
    pub fn register(&mut self, channel: Box<dyn Channel>) {
        let id = channel.id();
        self.channels.insert(id, channel);
    }

    /// Register an alias for a channel
    pub fn register_alias(&mut self, name: impl Into<String>, id: ChannelId) {
        self.aliases.insert(name.into(), id);
    }

    /// Resolve channel reference (fd number or alias) to ChannelId
    pub fn resolve(&self, reference: &str) -> Option<ChannelId> {
        // Try as number first
        if let Ok(id) = reference.parse::<ChannelId>() {
            if self.channels.contains_key(&id) {
                return Some(id);
            }
        }

        // Try as alias
        self.aliases.get(reference).copied()
    }

    /// Get immutable channel reference
    pub fn get(&self, id: ChannelId) -> Option<&dyn Channel> {
        self.channels.get(&id).map(|c| c.as_ref())
    }

    /// Get mutable channel reference
    pub fn get_mut(&mut self, id: ChannelId) -> Option<&mut (dyn Channel + '_)> {
        match self.channels.get_mut(&id) {
            Some(boxed) => Some(boxed.as_mut()),
            None => None,
        }
    }

    /// Get channel by alias
    pub fn get_by_alias(&self, alias: &str) -> Option<&dyn Channel> {
        self.aliases.get(alias).and_then(|id| self.get(*id))
    }

    /// Get mutable channel by alias
    pub fn get_by_alias_mut(&mut self, alias: &str) -> Option<&mut (dyn Channel + '_)> {
        let id = self.aliases.get(alias).copied()?;
        self.get_mut(id)
    }

    /// List all registered channel IDs
    pub fn list_channels(&self) -> Vec<ChannelId> {
        let mut ids: Vec<_> = self.channels.keys().copied().collect();
        ids.sort();
        ids
    }

    /// List all registered aliases
    pub fn list_aliases(&self) -> Vec<(&str, ChannelId)> {
        self.aliases
            .iter()
            .map(|(name, id)| (name.as_str(), *id))
            .collect()
    }

    /// Write to a channel by ID
    pub fn write_to(&mut self, id: ChannelId, data: &[u8]) -> Result<usize> {
        self.get_mut(id)
            .ok_or(ChannelError::NotFound(id))?
            .write(data)
            .map_err(Into::into)
    }

    /// Write to a channel by alias
    pub fn write_to_alias(&mut self, alias: &str, data: &[u8]) -> Result<usize> {
        let id = self
            .resolve(alias)
            .ok_or_else(|| ChannelError::InvalidOperation(format!("Unknown alias: {}", alias)))?;

        self.write_to(id, data)
    }
}

impl Default for ChannelManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alias_resolution() {
        let manager = ChannelManager::new();

        assert_eq!(manager.resolve("ctx"), Some(STDCONTEXT));
        assert_eq!(manager.resolve("context"), Some(STDCONTEXT));
        assert_eq!(manager.resolve("llm"), Some(STDLLM));
        assert_eq!(manager.resolve("tool"), Some(STDTOOL));
        assert_eq!(manager.resolve("trace"), Some(STDTRACE));
    }

    #[test]
    fn test_fd_resolution() {
        let manager = ChannelManager::new();

        // Only STDCONTEXT (3) is registered as an actual channel
        // Others are aliased but not yet implemented (Phase 2)
        assert_eq!(manager.resolve("3"), Some(STDCONTEXT));
        // Fd resolution for unregistered channels returns None
        assert_eq!(manager.resolve("4"), None);
        assert_eq!(manager.resolve("5"), None);
        assert_eq!(manager.resolve("6"), None);
    }

    #[test]
    fn test_list_aliases() {
        let manager = ChannelManager::new();
        let aliases = manager.list_aliases();

        assert!(aliases.len() >= 5); // At least ctx, context, llm, tool, trace
    }
}
