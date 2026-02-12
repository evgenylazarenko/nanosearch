use std::collections::HashMap;

/// An event that can be stored and replayed.
#[derive(Debug, Clone)]
pub struct Event {
    pub id: u64,
    pub name: String,
    pub payload: HashMap<String, String>,
}

/// Possible errors when interacting with the event store.
#[derive(Debug)]
pub enum EventStoreError {
    NotFound,
    DuplicateId,
    StorageFull,
}

/// Trait for types that can be serialized into events.
pub trait Serializable {
    fn to_event(&self) -> Event;
    fn from_event(event: &Event) -> Self;
}

/// Stores and retrieves events by ID.
pub struct EventStore {
    events: HashMap<u64, Event>,
    max_capacity: usize,
}

impl EventStore {
    pub fn new(max_capacity: usize) -> Self {
        EventStore {
            events: HashMap::new(),
            max_capacity,
        }
    }

    pub fn append(&mut self, event: Event) -> Result<(), EventStoreError> {
        if self.events.len() >= self.max_capacity {
            return Err(EventStoreError::StorageFull);
        }
        if self.events.contains_key(&event.id) {
            return Err(EventStoreError::DuplicateId);
        }
        self.events.insert(event.id, event);
        Ok(())
    }

    pub fn get(&self, id: u64) -> Result<&Event, EventStoreError> {
        self.events.get(&id).ok_or(EventStoreError::NotFound)
    }

    pub fn count(&self) -> usize {
        self.events.len()
    }
}

const MAX_DEFAULT_CAPACITY: usize = 10_000;

type EventId = u64;
