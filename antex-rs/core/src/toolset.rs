use std::collections::HashMap;

use crate::ErrorKind;
use crate::MAX_SCHEMA_BYTES;
use crate::MAX_TOOLS;
use crate::ProviderError;
use crate::RawToolCall;
use crate::ToolCall;
use crate::ToolDefinition;
use crate::validation::error;

pub(crate) struct ToolSet {
    pub definitions: Vec<ToolDefinition>,
    validators: HashMap<String, jsonschema::Validator>,
}

impl ToolSet {
    pub fn new(definitions: Vec<ToolDefinition>) -> Result<Self, ProviderError> {
        if definitions.len() > MAX_TOOLS {
            return Err(error(ErrorKind::Limit, "too many enabled tools"));
        }
        let mut validators = HashMap::new();
        for definition in &definitions {
            if definition.name.is_empty()
                || definition.name.len() > 64
                || definition.description.len() > MAX_SCHEMA_BYTES
                || definition.parameters.to_string().len() > MAX_SCHEMA_BYTES
            {
                return Err(error(
                    ErrorKind::Limit,
                    "tool definition exceeds its byte budget",
                ));
            }
            let validator = jsonschema::validator_for(&definition.parameters)
                .map_err(|_| error(ErrorKind::Protocol, "tool has an invalid input schema"))?;
            if validators
                .insert(definition.name.clone(), validator)
                .is_some()
            {
                return Err(error(ErrorKind::Protocol, "duplicate tool name"));
            }
        }
        Ok(Self {
            definitions,
            validators,
        })
    }

    pub fn validate(&self, call: &RawToolCall) -> Result<ToolCall, &'static str> {
        let validator = self
            .validators
            .get(&call.name)
            .ok_or("tool is not enabled for this turn")?;
        let arguments = serde_json::from_str(&call.arguments)
            .map_err(|_| "tool arguments are not valid JSON")?;
        if !validator.is_valid(&arguments) {
            return Err("tool arguments do not match the input schema");
        }
        Ok(ToolCall {
            id: call.id.clone(),
            name: call.name.clone(),
            arguments,
        })
    }
}
