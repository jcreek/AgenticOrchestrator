/// Full names of the installed fonts, e.g. `CaskaydiaCove Nerd Font Mono Bold (TrueType)`; empty if they cannot be read.
pub fn installed_font_names() -> Vec<String> {
    platform::font_names()
}

#[cfg(windows)]
mod platform {
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    const FONT_KEYS: [&str; 2] = [
        r"HKCU\Software\Microsoft\Windows NT\CurrentVersion\Fonts",
        r"HKLM\Software\Microsoft\Windows NT\CurrentVersion\Fonts",
    ];
    const VALUE_TYPE_COLUMN: &str = "    REG_SZ    ";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub(super) fn font_names() -> Vec<String> {
        FONT_KEYS.iter().flat_map(|key| registry_value_names(key)).collect()
    }

    fn registry_value_names(key: &str) -> Vec<String> {
        let system_root = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        let listing = Command::new(system_root.join("System32").join("reg.exe"))
            .args(["query", key])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let Ok(listing) = listing else {
            return Vec::new();
        };
        String::from_utf8_lossy(&listing.stdout)
            .lines()
            .filter_map(|line| line.split_once(VALUE_TYPE_COLUMN))
            .map(|(name, _)| name.trim().to_owned())
            .collect()
    }
}

#[cfg(target_os = "macos")]
use macos as platform;

#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    pub(super) fn font_names() -> Vec<String> {
        let Ok(output) = std::process::Command::new("fc-list")
            .args(["--format", "%{family}\n"]).env_clear().envs(crate::environment::environment()).output() else { return Vec::new(); };
        if !output.status.success() { return Vec::new(); }
        String::from_utf8_lossy(&output.stdout).lines()
            .flat_map(|line| line.split(',')).map(|name| name.trim().to_owned()).collect()
    }
}

/// Family suffixes Nerd Fonts use, longest first so `Mono` is not read as a style, with the preferred variant ranked lowest.
const FAMILY_SUFFIXES: [(&str, u8); 4] = [(" Nerd Font Mono", 1), (" Nerd Font", 0), (" NFM", 3), (" NF", 2)];
const PROPORTIONAL_VARIANT: &str = " Propo";

fn nerd_family(font_name: &str) -> Option<(u8, &str)> {
    FAMILY_SUFFIXES.iter().find_map(|(suffix, rank)| {
        let end = font_name.find(suffix)? + suffix.len();
        let rest = &font_name[end..];
        let ends_family = rest.is_empty() || rest.starts_with(' ');
        (ends_family && !rest.starts_with(PROPORTIONAL_VARIANT)).then_some((*rank, &font_name[..end]))
    })
}

/// Nerd Font builds of the terminal's built-in font, which keep its look.
const CASCADIA_FAMILIES: [&str; 2] = ["Cascadia Mono NF", "Cascadia Code NF"];

/// The installed Nerd Font family best suited to a terminal, if any.
pub fn nerd_font_family<S: AsRef<str>>(font_names: &[S]) -> Option<String> {
    let installed = |family: &str| font_names.iter().any(|name| name.as_ref().starts_with(family));
    if let Some(cascadia) = CASCADIA_FAMILIES.into_iter().find(|family| installed(family)) {
        return Some(cascadia.to_owned());
    }
    font_names
        .iter()
        .filter_map(|name| nerd_family(name.as_ref()))
        .min()
        .map(|(_, family)| family.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_plain_nerd_font_family_over_mono_and_abbreviated_names() {
        let installed = [
            "Arial (TrueType)",
            "MesloLGM NF (TrueType)",
            "CaskaydiaCove Nerd Font Mono Bold (TrueType)",
            "CaskaydiaCove Nerd Font SemiLight Italic (TrueType)",
        ];
        assert_eq!(nerd_font_family(&installed).as_deref(), Some("CaskaydiaCove Nerd Font"));
        assert_eq!(nerd_font_family(&installed[..3]).as_deref(), Some("CaskaydiaCove Nerd Font Mono"));
        assert_eq!(nerd_font_family(&installed[..2]).as_deref(), Some("MesloLGM NF"));
    }

    #[test]
    fn prefers_the_nerd_font_build_of_cascadia_mono() {
        let installed = ["MesloLGL Nerd Font Bold (TrueType)", "Cascadia Code NF Regular (TrueType)", "Cascadia Mono NF Regular (TrueType)"];
        assert_eq!(nerd_font_family(&installed).as_deref(), Some("Cascadia Mono NF"));
        assert_eq!(nerd_font_family(&installed[..2]).as_deref(), Some("Cascadia Code NF"));
    }

    #[test]
    fn ignores_proportional_variants_and_other_fonts() {
        let installed = ["Hack Nerd Font Propo Regular (TrueType)", "Hack NFP (TrueType)", "Consolas (TrueType)"];
        assert_eq!(nerd_font_family(&installed), None);
    }

    #[cfg(windows)]
    #[test]
    fn reads_font_names_from_the_registry() {
        assert!(installed_font_names().iter().any(|name| name.starts_with("Arial")));
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{CStr, c_char, c_void};
    type Ref = *const c_void;
    #[link(name = "CoreText", kind = "framework")]
    unsafe extern "C" { fn CTFontManagerCopyAvailableFontFamilyNames() -> Ref; }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFArrayGetCount(array: Ref) -> isize;
        fn CFArrayGetValueAtIndex(array: Ref, index: isize) -> Ref;
        fn CFStringGetLength(string: Ref) -> isize;
        fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
        fn CFStringGetCString(string: Ref, buffer: *mut c_char, size: isize, encoding: u32) -> bool;
        fn CFRelease(value: Ref);
    }
    pub(super) fn font_names() -> Vec<String> {
        // Copy returns an owned array; its string elements remain valid until CFRelease.
        unsafe {
            let array = CTFontManagerCopyAvailableFontFamilyNames();
            if array.is_null() { return Vec::new(); }
            let mut names = Vec::new();
            for index in 0..CFArrayGetCount(array) {
                let name = CFArrayGetValueAtIndex(array, index);
                let capacity = CFStringGetMaximumSizeForEncoding(CFStringGetLength(name), 0x08000100) + 1;
                if capacity <= 0 { continue; }
                let mut buffer = vec![0u8; capacity as usize];
                if CFStringGetCString(name, buffer.as_mut_ptr().cast(), capacity, 0x08000100) {
                    names.push(CStr::from_ptr(buffer.as_ptr().cast()).to_string_lossy().into_owned());
                }
            }
            CFRelease(array);
            names
        }
    }
}
