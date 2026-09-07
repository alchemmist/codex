use std::io;

use antex_core::ContextFragment;
use antex_core::ContextKind;
use antex_core::Message;
use serde_json::json;
use uuid::Uuid;

use super::Kind;
use super::Session;
use crate::session_codec;

impl Session {
    pub fn checkpoint(
        &mut self,
        summary: ContextFragment,
        retained: &[Uuid],
        tail_from: Uuid,
    ) -> io::Result<Option<Uuid>> {
        if summary.kind() != ContextKind::Summary || retained.len() > 8 {
            return Err(io::Error::other("invalid context checkpoint"));
        }
        let active = self.active_path()?;
        let tail = active
            .iter()
            .position(|entry| entry.record_id == tail_from)
            .ok_or_else(|| io::Error::other("checkpoint tail is not active"))?;
        let mut previous = None;
        for id in retained {
            let index = active
                .iter()
                .position(|entry| {
                    entry.record_id == *id && matches!(entry.message, Message::User(_))
                })
                .ok_or_else(|| io::Error::other("retained request is not active"))?;
            if index >= tail || previous.is_some_and(|previous| index <= previous) {
                return Err(io::Error::other("invalid retained request order"));
            }
            previous = Some(index);
        }
        let payload = json!({"type":"checkpoint","summary":session_codec::encode(&Message::Context(summary)),"retained":retained,"tail_from":tail_from});
        if let Some(first) = active.first()
            && self.records[&first.record_id].kind == Kind::Summary
            && self.read_record(first.record_id)?.0.payload == payload
        {
            return Ok(None);
        }
        self.append_record(Kind::Summary, payload, Some(self.head))
            .map(Some)
    }

    pub(super) fn selected_records(&mut self) -> io::Result<Vec<Uuid>> {
        let mut ids = Vec::new();
        let mut next = Some(self.head);
        while let Some(id) = next {
            ids.push(id);
            next = self
                .records
                .get(&id)
                .ok_or_else(|| io::Error::other("missing session parent"))?
                .parent;
        }
        ids.reverse();
        for position in (0..ids.len()).rev() {
            let id = ids[position];
            if self.records[&id].kind != Kind::Summary {
                continue;
            }
            let (record, _) = self.read_record(id)?;
            if record.payload["type"] != "checkpoint" {
                continue;
            }
            let tail_from: Uuid = serde_json::from_value(record.payload["tail_from"].clone())
                .map_err(|_| io::Error::other("invalid checkpoint tail"))?;
            let tail = ids[..position]
                .iter()
                .position(|id| *id == tail_from)
                .ok_or_else(|| io::Error::other("checkpoint tail is not an ancestor"))?;
            let retained = record.payload["retained"]
                .as_array()
                .filter(|ids| ids.len() <= 8)
                .ok_or_else(|| io::Error::other("invalid checkpoint retained requests"))?;
            let mut selected = vec![id];
            let mut previous = None;
            for value in retained {
                let retained: Uuid = serde_json::from_value(value.clone())
                    .map_err(|_| io::Error::other("invalid retained request identifier"))?;
                let index = ids[..tail]
                    .iter()
                    .position(|id| *id == retained && self.records[id].kind == Kind::User)
                    .ok_or_else(|| io::Error::other("retained request is not an ancestor"))?;
                if previous.is_some_and(|previous| index <= previous) {
                    return Err(io::Error::other("invalid retained request order"));
                }
                previous = Some(index);
                selected.push(retained);
            }
            selected.extend(
                ids[tail..]
                    .iter()
                    .copied()
                    .filter(|candidate| *candidate != id),
            );
            return Ok(selected);
        }
        Ok(ids)
    }
}
