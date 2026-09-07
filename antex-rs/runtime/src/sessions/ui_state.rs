use super::*;

impl Session {
    pub fn load_ui_state(&mut self, name: &str) -> io::Result<Option<Value>> {
        let mut next = Some(self.head);
        while let Some(id) = next {
            let index = &self.records[&id];
            next = index.parent;
            if index.kind == Kind::UiState {
                let (record, _) = self.read_record(id)?;
                if record.payload["name"] == name {
                    return Ok((!record.payload["value"].is_null())
                        .then(|| record.payload["value"].clone()));
                }
            }
        }
        Ok(None)
    }

    pub fn save_ui_state(&mut self, name: &str, value: &Value) -> io::Result<()> {
        if name.is_empty()
            || name.len() > 64
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            || value.to_string().len() > 16 * 1024 * 1024
        {
            return Err(io::Error::other("UI state exceeds its budget"));
        }
        self.append_record(
            Kind::UiState,
            json!({"name":name,"value":value}),
            Some(self.head),
        )?;
        self.finish_turn()
    }
}
