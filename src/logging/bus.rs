//! Event bus for dispatching events to multiple consumers.
//!
//! The `EventBus` is the central event distribution mechanism. Any subsystem
//! that produces events calls `emit()`, and all registered consumers receive
//! the event. Consumers are independent — they don't know about each other.
//!
//! # Primary consumers
//!
//! 1. **Event logger** — writes all events to JSONL for debugging/auditing
//! 2. **State recorder** — updates persistent state on step completion
//!    (currently called directly; will be converted to EventConsumer)
//! 3. **Presenter** — shows real-time terminal output
//!    (currently called directly; will be converted to EventConsumer)

use super::events::{BivvyEvent, EventConsumer};

/// Central event dispatcher that fans out events to registered consumers.
///
/// # Usage
///
/// ```
/// use bivvy::logging::bus::EventBus;
/// use bivvy::logging::{BivvyEvent, EventConsumer};
///
/// let mut bus = EventBus::new();
///
/// // In production, register the EventLogger:
/// // bus.add_consumer(Box::new(logger));
///
/// bus.emit(&BivvyEvent::StepStarting {
///     name: "build".to_string(),
/// });
/// ```
pub struct EventBus {
    consumers: Vec<Box<dyn EventConsumer>>,
}

impl EventBus {
    /// Create a new event bus with no consumers.
    pub fn new() -> Self {
        Self {
            consumers: Vec::new(),
        }
    }

    /// Register an event consumer.
    ///
    /// Consumers are called in registration order for each event.
    pub fn add_consumer(&mut self, consumer: Box<dyn EventConsumer>) {
        self.consumers.push(consumer);
    }

    /// Dispatch an event to all registered consumers.
    pub fn emit(&mut self, event: &BivvyEvent) {
        for consumer in &mut self.consumers {
            consumer.on_event(event);
        }
    }

    /// Returns the number of registered consumers.
    pub fn consumer_count(&self) -> usize {
        self.consumers.len()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Test consumer that records event type names.
    struct RecordingConsumer {
        events: Arc<Mutex<Vec<String>>>,
    }

    impl RecordingConsumer {
        fn new() -> (Self, Arc<Mutex<Vec<String>>>) {
            let events = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    events: events.clone(),
                },
                events,
            )
        }
    }

    impl EventConsumer for RecordingConsumer {
        fn on_event(&mut self, event: &BivvyEvent) {
            self.events
                .lock()
                .unwrap()
                .push(event.type_name().to_string());
        }
    }

    #[test]
    fn empty_bus_emits_without_error() {
        let mut bus = EventBus::new();
        bus.emit(&BivvyEvent::StepStarting {
            name: "test".to_string(),
        });
        assert_eq!(bus.consumer_count(), 0);
    }

    #[test]
    fn single_consumer_receives_events() {
        let mut bus = EventBus::new();
        let (consumer, events) = RecordingConsumer::new();
        bus.add_consumer(Box::new(consumer));

        bus.emit(&BivvyEvent::StepStarting {
            name: "build".to_string(),
        });
        bus.emit(&BivvyEvent::StepCompleted {
            name: "build".to_string(),
            success: true,
            exit_code: Some(0),
            duration_ms: 100,
            error: None,
        });

        let recorded = events.lock().unwrap();
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0], "step_starting");
        assert_eq!(recorded[1], "step_completed");
    }

    #[test]
    fn multiple_consumers_all_receive_events() {
        let mut bus = EventBus::new();
        let (consumer1, events1) = RecordingConsumer::new();
        let (consumer2, events2) = RecordingConsumer::new();
        bus.add_consumer(Box::new(consumer1));
        bus.add_consumer(Box::new(consumer2));

        bus.emit(&BivvyEvent::WorkflowStarted {
            name: "default".to_string(),
            step_count: 3,
        });

        assert_eq!(events1.lock().unwrap().len(), 1);
        assert_eq!(events2.lock().unwrap().len(), 1);
        assert_eq!(bus.consumer_count(), 2);
    }

    #[test]
    fn default_creates_empty_bus() {
        let bus = EventBus::default();
        assert_eq!(bus.consumer_count(), 0);
    }
}
