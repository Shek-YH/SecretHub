//! Batches of automatic AI imports (design 2026-11-10 section 3): which items and variables a
//! batch added, so that "undo" removes exactly those and nothing else. Only ids, names and
//! counts are stored here; no values.

use crate::{GroupUpdate, VariableUpdate, VaultError, VaultService};
use secrethub_storage::ImportBatch;
use std::collections::BTreeMap;

/// What an undo removed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct UndoSummary {
    pub removed_items: usize,
    pub removed_variables: usize,
}

impl VaultService {
    /// Starts a batch and returns its id.
    pub fn begin_import_batch(&self) -> Result<String, VaultError> {
        self.require_key()?;
        let id = format!("batch-{}", uuid::Uuid::new_v4());
        self.database.create_import_batch(&id)?;
        Ok(id)
    }

    /// Records that the batch created `secret_id` (`variable` is `None`) or added one variable.
    pub fn record_batch_entry(
        &self,
        batch_id: &str,
        secret_id: &str,
        variable: Option<&str>,
    ) -> Result<(), VaultError> {
        self.require_key()?;
        self.database
            .add_batch_entry(batch_id, secret_id, variable.unwrap_or(""))
            .map_err(Into::into)
    }

    /// Stores the counts shown in the notice: new groups, appended variables, variables saved
    /// as a separate group, other new items.
    pub fn finish_import_batch(&self, batch_id: &str, counts: [i64; 4]) -> Result<(), VaultError> {
        self.require_key()?;
        self.database.set_batch_counts(batch_id, counts)?;
        Ok(())
    }

    pub fn open_import_batches(&self) -> Result<Vec<ImportBatch>, VaultError> {
        self.require_key()?;
        self.database.list_open_import_batches().map_err(Into::into)
    }

    pub fn dismiss_import_batch(&self, batch_id: &str) -> Result<(), VaultError> {
        self.require_key()?;
        self.database.dismiss_import_batch(batch_id)?;
        Ok(())
    }

    /// Removes what the batch added and nothing else: items it created (unless a later batch
    /// still builds on them, then only its own variables go) and variables it appended.
    pub fn undo_import_batch(&self, batch_id: &str) -> Result<UndoSummary, VaultError> {
        self.require_key()?;
        match self.database.get_import_batch(batch_id)? {
            Some(batch) if !batch.undone => {}
            _ => return Err(VaultError::InvalidField("batch".into())),
        }
        let mut per_secret: BTreeMap<String, (bool, Vec<String>)> = BTreeMap::new();
        for (secret_id, variable) in self.database.list_batch_entries(batch_id)? {
            let entry = per_secret.entry(secret_id).or_default();
            if variable.is_empty() {
                entry.0 = true;
            } else {
                entry.1.push(variable);
            }
        }
        let mut summary = UndoSummary::default();
        for (secret_id, (created, variables)) in per_secret {
            let shared = self
                .database
                .secret_in_other_batches(batch_id, &secret_id)?;
            if created && !shared {
                if self.database.delete_secret(&secret_id)? {
                    summary.removed_items += 1;
                }
                continue;
            }
            if variables.is_empty() {
                continue;
            }
            let Some(metadata) = self.database.get_metadata(&secret_id)? else {
                continue;
            };
            if metadata.value_type != "env_group" {
                continue;
            }
            let current = self.group_values(&secret_id, None)?;
            let kept: Vec<VariableUpdate> = current
                .iter()
                .filter(|(name, _, _)| !variables.contains(name))
                .map(|(name, value, secret)| VariableUpdate {
                    name: name.clone(),
                    secret: *secret,
                    value: (!*secret).then(|| value.clone()),
                    from: None,
                })
                .collect();
            summary.removed_variables += current.len() - kept.len();
            self.update_group(
                &secret_id,
                GroupUpdate {
                    name: metadata.name.clone(),
                    category: metadata.category.clone(),
                    variables: kept,
                    notes: None,
                    tags: metadata.tags.clone(),
                    favorite: metadata.favorite,
                    ai_values: metadata.ai_values,
                },
            )?;
        }
        self.database.mark_import_batch_undone(batch_id)?;
        self.database.record_audit(
            "ai_import_undone",
            None,
            None,
            "success",
            &serde_json::json!({
                "source": "desktop",
                "batch": batch_id,
                "items": summary.removed_items,
                "variables": summary.removed_variables
            })
            .to_string(),
        )?;
        Ok(summary)
    }
}
