#![no_std]

/// Configures the boot behavior of the bootloader.
#[non_exhaustive]
#[derive(Debug)]
pub struct BootConfig<'a> {
    pub framebuffer_height: Option<u64>,
    pub framebuffer_width: Option<u64>,
    pub log_level: LogLevel,
    pub command_line: Option<&'a str>,
    pub splash_path: Option<&'a str>,
    #[doc(hidden)]
    pub _test_sentinel: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Quiet,
    Error,
    Normal,
    Verbose,
}

impl Default for BootConfig<'_> {
    fn default() -> Self {
        Self {
            framebuffer_height: None,
            framebuffer_width: None,
            log_level: LogLevel::Normal,
            command_line: None,
            splash_path: None,
            _test_sentinel: 0,
        }
    }
}

impl BootConfig<'_> {
    pub fn from_str<'a>(s: &'a str) -> BootConfig<'a> {
        let mut config: BootConfig<'a> = Default::default();

        for line in s.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue; // Skip empty lines and comments
            }

            let mut parts = line.splitn(2, '=');
            let key = parts.next().unwrap().trim();
            let value = parts.next().unwrap_or("").trim();

            match key {
                "framebuffer_height" => {
                    config.framebuffer_height = value.parse().ok();
                }
                "framebuffer_width" => {
                    config.framebuffer_width = value.parse().ok();
                }
                "log_level" => {
                    config.log_level = match value {
                        "quiet" => LogLevel::Quiet,
                        "error" => LogLevel::Error,
                        "normal" => LogLevel::Normal,
                        "verbose" => LogLevel::Verbose,
                        _ => LogLevel::Normal, // Default to Normal for unrecognized values
                    };
                }
                "command_line" => {
                    config.command_line = Some(value.trim_matches('"')); // Remove surrounding quotes if present
                }
                "splash_path" => {
                    config.splash_path = Some(value.trim_matches('"'));
                }
                _ => {
                    // Ignore unrecognized keys
                }
            }
        }

        config
    }
}
