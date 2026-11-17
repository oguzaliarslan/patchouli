//! Keyword table (`detail/keyword.hpp`). Lookups ignore ASCII case.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordKind {
    AudioChannels,
    AudioCodec,
    AudioLanguage,
    Device,
    Episode,
    EpisodeType,
    Language,
    Other,
    Part,
    ReleaseGroup,
    ReleaseInformation,
    ReleaseVersion,
    Season,
    Source,
    Subtitles,
    SubtitleLanguage,
    Type,
    VideoCodec,
    VideoColorDepth,
    VideoDynamicRange,
    VideoFormat,
    VideoFrameRate,
    VideoProfile,
    VideoQuality,
    VideoResolution,
    Volume,
}

pub const AMBIGUOUS: u8 = 1 << 0;
pub const SUBWORD: u8 = 1 << 1;
pub const PREFIX_FOR_NUMBER: u8 = 1 << 2;
pub const PREFIX_FOR_OTHER: u8 = 1 << 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Keyword {
    pub kind: KeywordKind,
    pub flags: u8,
}

impl Keyword {
    pub fn is_ambiguous(&self) -> bool {
        self.flags & AMBIGUOUS != 0
    }
    pub fn is_subword(&self) -> bool {
        self.flags & SUBWORD != 0
    }
    pub fn is_prefix_for_number(&self) -> bool {
        self.flags & PREFIX_FOR_NUMBER != 0
    }
    pub fn is_prefix_for_other(&self) -> bool {
        self.flags & PREFIX_FOR_OTHER != 0
    }
}

pub struct Keywords {
    map: HashMap<String, Keyword>,
    /// Lowercased prefixes of every key, for `has_candidates`.
    prefixes: HashSet<String>,
}

impl Keywords {
    pub fn get(&self, key: &str) -> Option<Keyword> {
        self.map.get(&key.to_ascii_lowercase()).copied()
    }

    /// Some keyword starts with `prefix`.
    pub fn has_candidates(&self, prefix: &str) -> bool {
        self.prefixes.contains(&prefix.to_ascii_lowercase())
    }
}

pub static KEYWORDS: LazyLock<Keywords> = LazyLock::new(build);

fn build() -> Keywords {
    use KeywordKind::*;
    #[rustfmt::skip]
    let table: &[(KeywordKind, &[(&str, u8)])] = &[
        (AudioChannels, &[
            ("2.0", AMBIGUOUS), ("2.0ch", 0), ("2ch", 0), ("5.1", 0), ("5.1ch", 0), ("7.1", 0),
            ("7.1ch", 0),
        ]),
        (AudioCodec, &[
            ("AAC", PREFIX_FOR_OTHER), ("AACx2", 0), ("AACx3", 0), ("AACx4", 0), ("AC3", 0),
            ("EAC3", 0), ("E-AC-3", 0), ("E-AC3", 0), ("Atmos", 0), ("Dolby Atmos", 0),
            ("DD", PREFIX_FOR_OTHER), ("DDP", PREFIX_FOR_NUMBER), ("Dolby TrueHD", 0),
            ("TrueHD", PREFIX_FOR_NUMBER), ("DTS", PREFIX_FOR_NUMBER), ("DTS-ES", 0),
            ("FLAC", PREFIX_FOR_NUMBER), ("FLACx2", 0), ("FLACx3", 0), ("FLACx4", 0),
            ("Lossless", 0), ("MP3", 0), ("Opus", AMBIGUOUS), ("OGG", 0), ("Vorbis", 0),
        ]),
        (AudioLanguage, &[
            ("DualAudio", 0), ("Dual Audio", 0), ("MultiAudio", 0), ("Multi Audio", 0), ("Dub", 0),
            ("Dubbed", 0), ("Dubs", 0), ("ChiDub", 0), ("Chinese Dub", 0), ("EngDub", 0),
            ("English Dub", 0), ("GerDub", 0), ("German Dub", 0), ("JapDub", 0),
            ("Japanese Dub", 0), ("Korean Dub", 0),
        ]),
        (Device, &[
            ("Android", AMBIGUOUS), ("iPad", PREFIX_FOR_NUMBER), ("iPhone", PREFIX_FOR_NUMBER),
            ("iPod", 0), ("PS", PREFIX_FOR_NUMBER), ("Xbox", PREFIX_FOR_NUMBER),
        ]),
        (Episode, &[
            ("Ep", PREFIX_FOR_NUMBER), ("Eps", PREFIX_FOR_NUMBER), ("Episode", PREFIX_FOR_NUMBER),
            ("Episodes", PREFIX_FOR_NUMBER), ("Episodio", PREFIX_FOR_NUMBER),
            ("Episódio", PREFIX_FOR_NUMBER), ("Capitulo", PREFIX_FOR_NUMBER),
            ("Folge", PREFIX_FOR_NUMBER),
        ]),
        (EpisodeType, &[
            ("OP", AMBIGUOUS | PREFIX_FOR_NUMBER), ("Opening", AMBIGUOUS),
            ("NCOP", PREFIX_FOR_NUMBER), ("ED", AMBIGUOUS | PREFIX_FOR_NUMBER),
            ("Ending", AMBIGUOUS), ("NCED", PREFIX_FOR_NUMBER), ("Preview", AMBIGUOUS),
            ("PV", AMBIGUOUS | PREFIX_FOR_NUMBER),
        ]),
        (Language, &[
            ("CHS", 0), ("CHT", 0), ("ENG", 0), ("English", 0), ("ESP", AMBIGUOUS),
            ("Espanol", 0), ("Spanish", 0), ("ITA", AMBIGUOUS), ("JAP", 0), ("JPN", 0),
            ("PT-BR", 0), ("VOSTFR", 0),
        ]),
        (Other, &[
            ("Remaster", 0), ("Remastered", 0), ("Uncensored", 0), ("Uncut", 0), ("TS", 0),
            ("VFR", 0), ("Widescreen", 0), ("WS", 0),
        ]),
        (Part, &[
            ("Cour", PREFIX_FOR_NUMBER), ("Part", AMBIGUOUS | PREFIX_FOR_NUMBER),
            ("Parte", PREFIX_FOR_NUMBER),
        ]),
        (ReleaseGroup, &[("0x539", 0), ("THORA", 0), ("VARYG", 0)]),
        (ReleaseInformation, &[
            ("Batch", 0), ("Complete", 0), ("End", AMBIGUOUS), ("Final", AMBIGUOUS), ("Patch", 0),
            ("Remux", 0), ("Repack", 0),
        ]),
        (ReleaseVersion, &[("v0", 0), ("v1", 0), ("v2", 0), ("v3", 0), ("v4", 0)]),
        (Season, &[("Season", AMBIGUOUS), ("Saison", AMBIGUOUS)]),
        (Source, &[
            ("BD", PREFIX_FOR_OTHER), ("BDRip", 0), ("BluRay", 0), ("Blu ray", 0),
            ("DVD", PREFIX_FOR_NUMBER), ("DVD5", 0), ("DVD9", 0), ("DVDISO", 0), ("DVDRip", 0),
            ("DVD Rip", 0), ("R2DVD", 0), ("R2J", 0), ("R2JDVD", 0), ("R2JDVDRip", 0), ("HDTV", 0),
            ("HDTVRip", 0), ("TVRip", 0), ("TV Rip", 0), ("Web", AMBIGUOUS), ("Webcast", 0),
            ("WebDL", 0), ("Web DL", 0), ("WebRip", 0), ("ADN", 0), ("AMZN", 0), ("BILI", 0),
            ("Bilibili", 0), ("CR", 0), ("Crunchyroll", 0), ("DSNP", 0), ("Funi", 0),
            ("Funimation", 0), ("HIDI", 0), ("Hidive", 0), ("Hulu", 0), ("Netflix", 0), ("NF", 0),
            ("VRV", 0), ("YouTube", 0),
        ]),
        (Subtitles, &[
            ("ASS", 0), ("BIG5", 0), ("Hardsub", 0), ("Hardsubs", 0), ("RAW", 0), ("Softsub", 0),
            ("Softsubs", 0), ("Sub", 0), ("Subbed", 0), ("Subtitled", 0), ("Multisub", 0),
            ("Multi Sub", 0), ("Multi Subs", 0), ("Multiple Subtitle", 0),
        ]),
        (SubtitleLanguage, &[("EngSub", 0), ("EngSubs", 0), ("GerSub", 0)]),
        (Type, &[
            ("TV", AMBIGUOUS), ("Movie", AMBIGUOUS), ("Gekijouban", AMBIGUOUS),
            ("OAD", AMBIGUOUS | PREFIX_FOR_NUMBER), ("OAV", AMBIGUOUS | PREFIX_FOR_NUMBER),
            ("OVA", AMBIGUOUS | PREFIX_FOR_NUMBER), ("ONA", AMBIGUOUS | PREFIX_FOR_NUMBER),
            ("SP", AMBIGUOUS | PREFIX_FOR_NUMBER), ("Special", AMBIGUOUS),
            ("Specials", AMBIGUOUS),
        ]),
        (VideoColorDepth, &[
            ("8bit", 0), ("8bits", 0), ("8 bit", 0), ("8 bits", 0), ("10bit", 0), ("10bits", 0),
            ("10 bit", 0), ("10 bits", 0),
        ]),
        (VideoCodec, &[
            ("AV1", 0), ("DivX", PREFIX_FOR_NUMBER), ("AVC", 0), ("H 264", 0), ("H264", 0),
            ("X 264", 0), ("X264", 0), ("H 265", 0), ("H265", 0), ("HEVC", PREFIX_FOR_NUMBER),
            ("X 265", 0), ("X265", 0), ("Xvid", 0),
        ]),
        (VideoDynamicRange, &[("HDR", 0), ("HDR10", 0), ("Dolby Vision", 0), ("DV", 0)]),
        (VideoFormat, &[
            ("AVI", 0), ("MP4", 0), ("RMVB", 0), ("WMV", 0), ("WMV3", 0), ("WMV9", 0),
        ]),
        (VideoFrameRate, &[
            ("23.976FPS", 0), ("24FPS", 0), ("29.97FPS", 0), ("30FPS", 0), ("60FPS", 0),
            ("120FPS", 0),
        ]),
        (VideoProfile, &[
            ("Hi10", 0), ("Hi10p", 0), ("Hi444", 0), ("Hi444P", 0), ("Hi444PP", 0),
        ]),
        (VideoQuality, &[("HD", 0), ("SD", 0), ("HQ", 0), ("LQ", 0)]),
        (VideoResolution, &[
            ("1080p", SUBWORD), ("1440p", SUBWORD), ("2160p", SUBWORD), ("4K", 0),
        ]),
        (Volume, &[("Vol", PREFIX_FOR_NUMBER), ("Volume", PREFIX_FOR_NUMBER)]),
    ];

    let mut map = HashMap::new();
    for &(kind, entries) in table {
        for &(value, flags) in entries {
            let keyword = Keyword { kind, flags };
            // Like `emplace`: the first entry for a key wins.
            map.entry(value.to_ascii_lowercase()).or_insert(keyword);
            if value.contains(' ') {
                for delimiter in ['_', '.', '-'] {
                    map.entry(
                        value
                            .replace(' ', &delimiter.to_string())
                            .to_ascii_lowercase(),
                    )
                    .or_insert(keyword);
                }
            }
        }
    }
    let prefixes = map
        .keys()
        .flat_map(|key| {
            key.char_indices()
                .map(|(i, c)| key[..i + c.len_utf8()].to_owned())
                .collect::<Vec<_>>()
        })
        .collect();
    Keywords { map, prefixes }
}
