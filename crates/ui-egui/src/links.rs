//! Project links and provenance notes: the About dialog, the Help tab and the File tab open or show
//! these. This build is a fork of storytold/wordcraft; its links point at the fork's repository.

/// The fork's repository (Help › GitHub, the About dialog's source link).
pub const GITHUB: &str = "https://github.com/685ks/wordcraft";
/// The project this build is a fork of (named in the About dialog).
pub const UPSTREAM: &str = "https://github.com/storytold/wordcraft";
/// The About dialog's provenance line. A legal notice, shown in English in every interface language.
pub const FORK_NOTE: &str = "A fork of storytold/wordcraft, not affiliated with or endorsed by ArtCraft.";
/// Shown in the About dialog when the Arthavan Projects launcher started the app
/// (`ARTHAVAN_LAUNCHER=1`). WordCraft has no updater of its own and makes no update requests; the
/// launcher installs and updates it from the 685ks/wordcraft releases.
pub const LAUNCHER_UPDATES_NOTE: &str = "Updates are managed by Arthavan Projects";

/// Whether the Arthavan Projects launcher started this process (`ARTHAVAN_LAUNCHER=1`).
pub fn launcher_managed() -> bool {
    launcher_managed_from(std::env::var("ARTHAVAN_LAUNCHER").ok().as_deref())
}

fn launcher_managed_from(value: Option<&str>) -> bool {
    value.is_some_and(|v| v.trim() == "1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_flag_is_read_from_arthavan_launcher() {
        assert!(launcher_managed_from(Some("1")));
        assert!(launcher_managed_from(Some(" 1\n")));
        assert!(!launcher_managed_from(None));
        assert!(!launcher_managed_from(Some("0")));
        assert!(!launcher_managed_from(Some("")));
        assert!(!launcher_managed_from(Some("true")));
    }

    /// This fork carries no ArtCraft community or website links (house rebrand rule).
    #[test]
    fn no_link_or_command_points_at_artcraft_services() {
        for url in [GITHUB, UPSTREAM] {
            for banned in ["artcraft", "discord", "storyteller.ai"] {
                assert!(!url.to_ascii_lowercase().contains(banned), "{url}");
            }
        }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "rs") || path.ends_with("links.rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap().to_ascii_lowercase();
            for banned in ["getartcraft", "discord.gg", "ui.discord"] {
                assert!(!src.contains(banned), "{} mentions {banned}", path.display());
            }
        }
        let i18n = dir.join("i18n");
        for entry in std::fs::read_dir(&i18n).unwrap() {
            let path = entry.unwrap().path();
            let src = std::fs::read_to_string(&path).unwrap().to_ascii_lowercase();
            for banned in ["getartcraft", "discord", "crafting apps from the artcraft"] {
                assert!(!src.contains(banned), "{} mentions {banned}", path.display());
            }
        }
    }
}
