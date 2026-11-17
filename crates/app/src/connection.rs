//! Where to find the robot: the address modes Elastic offers, with the same
//! defaults, so drivers see familiar choices.

/// How the dashboard picks the robot's address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressMode {
    /// Ask the Driver Station on this computer (websocket port 6768).
    DriverStation,
    /// `10.TE.AM.2` from the team number.
    TeamNumber,
    /// SystemCore over mDNS: `robot.local`.
    SystemCoreMdns,
    /// SystemCore's own WiFi access point: `172.30.0.1`.
    SystemCoreWifi,
    /// The simulator on this computer.
    Localhost,
    /// Typed in by hand.
    Custom,
}

impl AddressMode {
    /// Every mode, in the order the settings menu shows them.
    pub const ALL: [AddressMode; 6] = [
        AddressMode::DriverStation,
        AddressMode::TeamNumber,
        AddressMode::SystemCoreMdns,
        AddressMode::SystemCoreWifi,
        AddressMode::Localhost,
        AddressMode::Custom,
    ];

    /// Name shown in the menu.
    pub fn label(self) -> &'static str {
        match self {
            AddressMode::DriverStation => "Driver Station",
            AddressMode::TeamNumber => "Team Number (10.TE.AM.2)",
            AddressMode::SystemCoreMdns => "SystemCore mDNS",
            AddressMode::SystemCoreWifi => "SystemCore WiFi",
            AddressMode::Localhost => "localhost (sim)",
            AddressMode::Custom => "Custom",
        }
    }
}

/// The robot radio address for a team: team 3200 is `10.32.0.2`.
///
/// ```
/// # use raptaboard::connection::team_address;
/// assert_eq!(team_address(3200), "10.32.0.2");
/// assert_eq!(team_address(254), "10.2.54.2");
/// ```
pub fn team_address(team: u32) -> String {
    format!("10.{}.{}.2", team / 100, team % 100)
}

/// The host to connect to, or `None` while the Driver Station mode has not
/// reported an address yet (that lookup is not built yet).
pub fn resolve(mode: AddressMode, team: u32, custom: &str) -> Option<String> {
    match mode {
        AddressMode::DriverStation => None,
        AddressMode::TeamNumber => Some(team_address(team)),
        AddressMode::SystemCoreMdns => Some("robot.local".into()),
        AddressMode::SystemCoreWifi => Some("172.30.0.1".into()),
        AddressMode::Localhost => Some("127.0.0.1".into()),
        AddressMode::Custom => Some(custom.trim().to_owned()),
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    #[test]
    fn modes_resolve_like_elastic() {
        assert_eq!(
            resolve(AddressMode::TeamNumber, 3200, ""),
            Some("10.32.0.2".into())
        );
        assert_eq!(
            resolve(AddressMode::SystemCoreMdns, 0, ""),
            Some("robot.local".into())
        );
        assert_eq!(
            resolve(AddressMode::SystemCoreWifi, 0, ""),
            Some("172.30.0.1".into())
        );
        assert_eq!(
            resolve(AddressMode::Localhost, 0, ""),
            Some("127.0.0.1".into())
        );
        assert_eq!(
            resolve(AddressMode::Custom, 0, " 10.0.0.5 "),
            Some("10.0.0.5".into())
        );
        assert_eq!(resolve(AddressMode::DriverStation, 3200, ""), None);
    }
}
