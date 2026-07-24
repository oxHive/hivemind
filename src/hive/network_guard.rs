use crate::hive::network::TrustedNetwork;

/// What the guard loop should do this tick, given whether the hive stack is
/// currently running, the trusted-network allowlist, and the current
/// network's identity key (`None` when unidentifiable, e.g. `whichnet`
/// returned `Unknown` or this platform isn't supported).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum GuardAction {
    None,
    Pause,
    Resume,
}

/// Pure decision function for the pause/resume state machine -- kept free of
/// any spawning/aborting side effects so it can be tested without real
/// listeners or mDNS. An empty trusted list means the feature is off: the
/// stack is left exactly as it was started (today's always-on behavior).
pub(crate) fn decide_action(
    stack_running: bool,
    trusted: &[TrustedNetwork],
    current: Option<&str>,
) -> GuardAction {
    if trusted.is_empty() {
        return GuardAction::None;
    }
    let is_trusted = current
        .map(|c| trusted.iter().any(|t| t.id == c))
        .unwrap_or(false);
    match (stack_running, is_trusted) {
        (true, false) => GuardAction::Pause,
        (false, true) => GuardAction::Resume,
        _ => GuardAction::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trusted(id: &str) -> TrustedNetwork {
        TrustedNetwork { id: id.to_string(), label: None, added_at: 0 }
    }

    #[test]
    fn empty_allowlist_never_acts() {
        assert_eq!(decide_action(true, &[], Some("ssid:cafe")), GuardAction::None);
        assert_eq!(decide_action(false, &[], Some("ssid:home")), GuardAction::None);
        assert_eq!(decide_action(false, &[], None), GuardAction::None);
    }

    #[test]
    fn running_on_untrusted_network_pauses() {
        let list = [trusted("ssid:home")];
        assert_eq!(
            decide_action(true, &list, Some("ssid:cafe")),
            GuardAction::Pause
        );
    }

    #[test]
    fn running_with_unidentifiable_network_pauses() {
        let list = [trusted("ssid:home")];
        assert_eq!(decide_action(true, &list, None), GuardAction::Pause);
    }

    #[test]
    fn paused_on_trusted_network_resumes() {
        let list = [trusted("ssid:home")];
        assert_eq!(
            decide_action(false, &list, Some("ssid:home")),
            GuardAction::Resume
        );
    }

    #[test]
    fn paused_on_untrusted_network_stays_paused() {
        let list = [trusted("ssid:home")];
        assert_eq!(decide_action(false, &list, Some("ssid:cafe")), GuardAction::None);
    }

    #[test]
    fn running_on_trusted_network_keeps_running() {
        let list = [trusted("ssid:home")];
        assert_eq!(
            decide_action(true, &list, Some("ssid:home")),
            GuardAction::None
        );
    }
}
