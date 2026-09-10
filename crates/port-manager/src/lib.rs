use std::collections::HashMap;
use std::net::TcpListener;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Reservation {
    pub owner: String,
    pub expires_at: Instant,
}

#[derive(Debug)]
pub struct PortManager {
    reservations: HashMap<u16, Reservation>,
    ttl: Duration,
    start: u16,
    end: u16,
}

impl Default for PortManager {
    fn default() -> Self {
        Self::new(4000, 4999, Duration::from_secs(60))
    }
}

impl PortManager {
    pub fn new(start: u16, end: u16, ttl: Duration) -> Self {
        Self {
            reservations: HashMap::new(),
            ttl,
            start,
            end,
        }
    }

    pub fn reserve(&mut self, owner: impl Into<String>) -> Option<u16> {
        self.reap();
        let owner = owner.into();

        for port in self.start..=self.end {
            if self.reservations.contains_key(&port) || !is_available(port) {
                continue;
            }

            self.reservations.insert(
                port,
                Reservation {
                    owner: owner.clone(),
                    expires_at: Instant::now() + self.ttl,
                },
            );
            return Some(port);
        }

        None
    }

    pub fn release_owned(&mut self, port: u16, owner: &str) -> bool {
        self.reap();

        let owned = self
            .reservations
            .get(&port)
            .map(|reservation| reservation.owner == owner)
            .unwrap_or(false);

        if owned {
            self.reservations.remove(&port);
            true
        } else {
            false
        }
    }

    pub fn owner(&mut self, port: u16) -> Option<String> {
        self.reap();
        self.reservations
            .get(&port)
            .map(|reservation| reservation.owner.clone())
    }

    pub fn release(&mut self, port: u16) -> bool {
        self.reap();
        self.reservations.remove(&port).is_some()
    }

    pub fn is_reserved(&mut self, port: u16) -> bool {
        self.reap();
        self.reservations.contains_key(&port)
    }

    pub fn reap(&mut self) {
        let now = Instant::now();
        self.reservations
            .retain(|_, reservation| reservation.expires_at > now);
    }
}

fn is_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservation_can_be_released_by_owner() {
        let mut manager = PortManager::new(45000, 45100, Duration::from_secs(60));

        if let Some(port) = manager.reserve("session-a") {
            assert!(!manager.release_owned(port, "session-b"));
            assert!(manager.is_reserved(port));
            assert!(manager.release_owned(port, "session-a"));
            assert!(!manager.is_reserved(port));
        }
    }
}
