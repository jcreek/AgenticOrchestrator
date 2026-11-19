use std::time::Duration;

use serde::{Deserialize, Serialize};

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/jcreek/AgenticOrchestrator/releases/latest";
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub version: String,
    pub url: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
}

/// The latest published release when it is newer than `current`; `None` when it is not or GitHub cannot be reached.
pub fn available_update(current: &str) -> Option<Update> {
    let release = latest_release().ok()?;
    let version = newer_version(current, &release.tag_name)?;
    Some(Update { version, url: release.html_url })
}

fn latest_release() -> Result<Release, ureq::Error> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(TIMEOUT)).build().into();
    agent
        .get(LATEST_RELEASE_URL)
        .header("User-Agent", "AgenticOrchestrator")
        .header("Accept", "application/vnd.github+json")
        .call()?
        .body_mut()
        .read_json()
}

fn parse(version: &str) -> Option<[u64; 3]> {
    let mut parts = version.trim().trim_start_matches('v').split('.').map(|part| part.parse().ok());
    let parsed = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(parsed)
}

/// `latest_tag` without its `v` prefix when it is a newer `major.minor.patch` than `current`.
pub fn newer_version(current: &str, latest_tag: &str) -> Option<String> {
    let latest = parse(latest_tag)?;
    (latest > parse(current)?).then(|| latest.map(|part| part.to_string()).join("."))
}

#[cfg(test)]
mod tests {
    use super::newer_version;

    #[test]
    fn a_higher_tag_is_newer_comparing_numerically() {
        assert_eq!(newer_version("0.3.3", "v0.3.4").as_deref(), Some("0.3.4"));
        assert_eq!(newer_version("0.9.0", "0.10.0").as_deref(), Some("0.10.0"));
        assert_eq!(newer_version("0.3.3", "v1.0.0").as_deref(), Some("1.0.0"));
    }

    #[test]
    fn the_same_or_an_older_tag_is_not_newer() {
        assert_eq!(newer_version("0.3.3", "v0.3.3"), None);
        assert_eq!(newer_version("0.3.3", "v0.3.2"), None);
    }

    #[test]
    fn unparseable_versions_are_never_newer() {
        for tag in ["", "latest", "v0.4", "v0.4.0-beta", "v0.4.0.1"] {
            assert_eq!(newer_version("0.3.3", tag), None, "{tag:?}");
        }
        assert_eq!(newer_version("dev", "v9.9.9"), None);
    }
}
