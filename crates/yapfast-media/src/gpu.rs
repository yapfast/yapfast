//! Which of a machine's graphics adapters Yapfast makes its device on, and the
//! setting that decides it. A machine with a graphics card and a processor's
//! GPU has two adapters, and which one the device is made on decides which
//! hardware encoder is reachable — NVENC on NVIDIA's card, Quick Sync on
//! Intel's — and what a captured screen costs to reach it, a copy of every
//! frame unless the capture and the encoder are on the same adapter.
//!
//! Dedicated is the default: a game runs on the card, and NVENC is what
//! streaming was written for. The choice is made once, when the device is
//! made, so it costs nothing per frame.
//!
//! The encoders that device reaches are this module's too: [`Backend`] is the
//! family a vendor's driver provides, and [`candidates`] is the order they are
//! tried in for the adapter the preference picks and the one a capture filled
//! its frame on. `encode.rs` opens them in that order and [`select`] is the
//! answer to what happens when one doesn't.
use std::cmp::Reverse;
use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

/// Which of a machine's graphics adapters to make Yapfast's device on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preference {
    /// The card with memory of its own, NVIDIA's or AMD's.
    #[default]
    Dedicated,
    /// The GPU built into the processor, Intel's.
    Integrated,
}

/// DXGI's vendor ids for the makers of those GPUs.
const NVIDIA: u32 = 0x10DE;
const AMD: u32 = 0x1002;
const INTEL: u32 = 0x8086;

/// Where an adapter stands under a [`Preference`]. Ranks order best first, so
/// a loop over the adapters takes the smallest; an adapter with no rank is
/// never made a device on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    /// The adapter the preference asks for.
    Preferred,
    /// A hardware adapter of another kind, made the device on when the
    /// preferred kind isn't there, which leaves a machine with one GPU
    /// exactly as it was.
    Fallback,
}

/// DXGI's name for an adapter: locally unique, the same across runs and
/// enumerations, and what the device is asked for.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Luid {
    pub low: u32,
    pub high: i32,
}

/// What an enumerated adapter offers towards choosing it, as
/// `DXGI_ADAPTER_DESC1` gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Adapter {
    /// Which adapter this is, to make the device on it.
    pub luid: Luid,
    /// `VendorId`: who makes the GPU.
    pub vendor: u32,
    /// `DedicatedVideoMemory`: the memory it has of its own, in bytes.
    pub dedicated_memory: u64,
    /// `SharedSystemMemory`: how much of the system's memory it will use, in
    /// bytes. It says nothing about which kind of GPU a card is — an
    /// integrated one reports up to half of RAM here — so it doesn't decide
    /// anything.
    pub shared_memory: u64,
    /// `DXGI_ADAPTER_FLAG_SOFTWARE`: the Basic Render Driver, or a driver
    /// small enough to be run by the processor, drawing what a GPU would.
    pub software: bool,
}

impl Preference {
    /// Where `adapter` stands under this preference, or `None` for a software
    /// adapter, which is never made a device on: it has no encoder and no
    /// texture of its own.
    ///
    /// The kind of GPU is read from the vendor and never from the memory,
    /// because memory doesn't tell them apart: an Intel UHD reports a nominal
    /// share of dedicated memory beside half of RAM in `SharedSystemMemory`,
    /// and Arc, Intel's own card, reports the same shape with gigabytes of
    /// its own. A threshold on either would call the UHD a card or the Arc an
    /// integrated GPU, by how much system memory the machine has.
    pub fn rank(self, adapter: &Adapter) -> Option<Rank> {
        if adapter.software {
            return None;
        }
        let card = matches!(adapter.vendor, NVIDIA | AMD);
        let wanted = match self {
            Preference::Dedicated => card,
            Preference::Integrated => adapter.vendor == INTEL,
        };
        Some(if wanted {
            Rank::Preferred
        } else {
            Rank::Fallback
        })
    }
}

/// The adapter to make the device on: the best rank under `preference`, and of
/// two ranked alike, the one with more memory of its own. That only sorts two
/// adapters of the same kind, and anything still tied keeps the order DXGI
/// listed them in, which starts with the adapter the desktop shows on.
///
/// `None` only when there is no hardware adapter at all, so a machine with one
/// GPU behaves as it did before this setting existed.
pub fn choose(
    preference: Preference,
    adapters: impl IntoIterator<Item = Adapter>,
) -> Option<Adapter> {
    adapters
        .into_iter()
        .filter_map(|adapter| preference.rank(&adapter).map(|rank| (rank, adapter)))
        .min_by(|(rank, adapter), (best, best_adapter)| {
            rank.cmp(best).then(
                adapter
                    .dedicated_memory
                    .cmp(&best_adapter.dedicated_memory)
                    .reverse(),
            )
        })
        .map(|(_, adapter)| adapter)
}

/// A GPU maker's hardware encoder, as the families FFmpeg's build names them:
/// NVENC on NVIDIA's cards, AMF on AMD's on Windows, Quick Sync on Intel's on
/// Windows, and VA-API on AMD's and Intel's on Linux.
///
/// Every one of them is fed from system memory: FFmpeg's encoders here are
/// opened on an I420 picture, so a frame a capture filled on a GPU is
/// converted and uploaded by the processor before the encoder reads it. The
/// zero-copy path is NVIDIA's alone and is a different one — `d3d11.rs` and
/// native/d3d11enc.c hand NVENC a texture on the device the capture filled it
/// on — so a stream that goes through an encoder here is a stream whose every
/// frame crossed the bus, and its report says so rather than calling it
/// zero-copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Backend {
    Nvenc,
    Amf,
    Qsv,
    Vaapi,
}

impl Backend {
    /// The backends a GPU of `vendor` has on this platform, best first: only
    /// the ones a driver of that vendor really provides, since VA-API is
    /// Linux's and AMF and Quick Sync are Windows', so no card is asked for an
    /// encoder its platform hasn't got. Empty for a vendor with none. Intel's
    /// GPUs on Windows have Quick Sync, which this FFmpeg carries only when it
    /// was built with oneVPL: a name the build hasn't got is one `plan` leaves
    /// out, and the report says so for it.
    pub fn on(vendor: u32) -> &'static [Backend] {
        if cfg!(windows) {
            match vendor {
                NVIDIA => &[Backend::Nvenc],
                AMD => &[Backend::Amf],
                INTEL => &[Backend::Qsv],
                _ => &[],
            }
        } else {
            match vendor {
                // NVIDIA's own VA-API driver only decodes; NVENC encodes.
                NVIDIA => &[Backend::Nvenc],
                AMD | INTEL => &[Backend::Vaapi],
                _ => &[],
            }
        }
    }

    /// What it is called where a person reads it, for a report that has to say
    /// which encoder ran rather than which codec's name FFmpeg gave it.
    pub fn name(self) -> &'static str {
        match self {
            Backend::Nvenc => "NVENC",
            Backend::Amf => "AMF",
            Backend::Qsv => "Quick Sync",
            Backend::Vaapi => "VA-API",
        }
    }
}

/// What reaching a hardware encoder costs the picture. The encoders here read
/// system memory, so a frame is uploaded to the processor in every case; a
/// stream whose capture filled its frame on another adapter pays a copy across
/// the bus as well. A report says which of the two it is, so a fallback isn't
/// taken for the same-adapter path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reach {
    /// The encoder is on the adapter the capture filled the frame on, or the
    /// frame has no adapter of its own — a camera's, a clip's: one upload, and
    /// nothing crosses.
    Uploaded,
    /// The encoder is on another adapter than the capture's: the frame is
    /// uploaded and carried across, which is what a fallback costs.
    Crosses,
}

/// One hardware encoder this machine could run `codec` on: which backend, on
/// which adapter, how well the preference likes that adapter, and what
/// reaching it costs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub backend: Backend,
    /// The adapter it runs on; `None` when this machine's adapters couldn't be
    /// listed, which leaves the encoders in the order FFmpeg's build lists
    /// them.
    pub adapter: Option<Adapter>,
    /// Where that adapter stands under the preference that chose it.
    pub rank: Rank,
    pub reach: Reach,
}

/// The hardware encoders to try, best first, on a machine whose adapters are
/// `adapters` and whose capture fills its frames on `capture`.
///
/// The adapter the preference picks comes first, with the backends its vendor
/// has in turn; then, as fallbacks, the adapter the capture is on — the
/// same-adapter path, which automatic encoding would rather have than a copy —
/// and then the rest of the machine, the best-ranked first. A frame with no
/// adapter of its own ranks by the preference alone.
///
/// Empty when no hardware adapter could be listed, which is how a platform that
/// can't enumerate them keeps FFmpeg's own order, and when the machine has no
/// hardware adapter at all.
/// Whether this machine and this build can share a screen without the processor
/// copying every picture: the graphics card's own encoder reading the capture
/// where it lies. It is what the settings' note asks — so it opens nothing, and
/// answers from the build and the devices rather than from a driver's reply. A
/// card too old for the encoders, or a build without them, answers no, and the
/// note is never shown to the person using it.
pub fn shares_on_the_card() -> bool {
    #[cfg(target_os = "linux")]
    {
        if cfg!(any(nvcuda, dmabufenc)) {
            // A render node is what the card's captures and encoders open; a
            // machine without one has no card to share from.
            return std::fs::read_dir("/dev/dri").is_ok_and(|entries| {
                entries.filter_map(|entry| entry.ok()).any(|entry| {
                    entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| name.starts_with("renderD"))
                })
            });
        }
        false
    }
    #[cfg(not(target_os = "linux"))]
    {
        if cfg!(any(wgc, dup)) {
            return !machine().is_empty();
        }
        false
    }
}

pub fn candidates(
    preference: Preference,
    adapters: &[Adapter],
    capture: Option<Luid>,
) -> Vec<Candidate> {
    let Some(best) = choose(preference, adapters.iter().copied()) else {
        return Vec::new();
    };
    // The order adapters are asked in: the preference's, then the capture's
    // when that is another adapter, then everything else.
    let mut order = vec![best];
    let on_capture = capture.and_then(|luid| {
        adapters
            .iter()
            .copied()
            .find(|adapter| !adapter.software && adapter.luid == luid)
    });
    if let Some(on_capture) = on_capture.filter(|adapter| adapter.luid != best.luid) {
        order.push(on_capture);
    }
    let mut rest: Vec<Adapter> = adapters
        .iter()
        .copied()
        .filter(|adapter| !adapter.software && !order.iter().any(|had| had.luid == adapter.luid))
        .collect();
    // Two adapters alike in rank are told apart by their memory of their own,
    // as `choose` tells them apart.
    rest.sort_by_key(|adapter| {
        (
            preference.rank(adapter).unwrap_or(Rank::Fallback),
            Reverse(adapter.dedicated_memory),
        )
    });
    order.extend(rest);
    order
        .into_iter()
        .flat_map(|adapter| {
            Backend::on(adapter.vendor)
                .iter()
                .map(move |&backend| Candidate {
                    backend,
                    adapter: Some(adapter),
                    rank: preference.rank(&adapter).unwrap_or(Rank::Fallback),
                    reach: reach(capture, &adapter),
                })
        })
        .collect()
}

/// What reaching `adapter` from a frame the capture filled on `capture` costs.
fn reach(capture: Option<Luid>, adapter: &Adapter) -> Reach {
    match capture {
        Some(luid) if luid != adapter.luid => Reach::Crosses,
        _ => Reach::Uploaded,
    }
}

/// The first candidate that opens, which is the encoder that runs: the
/// preference's adapter where it can encode, then the same-adapter path, then
/// the rest of the machine. `opens` is asked about each in turn, so a probe
/// pays only for the opens it needs, and a candidate that doesn't open is a
/// machine's fact rather than a failure.
pub fn select(
    candidates: &[Candidate],
    mut opens: impl FnMut(&Candidate) -> bool,
) -> Option<Candidate> {
    candidates
        .iter()
        .copied()
        .find(|candidate| opens(candidate))
}

/// The machine's adapters as DXGI lists them, which starts with the one the
/// desktop is shown on; empty on a platform that can't list them, which leaves
/// the encoders in the order FFmpeg's build has them.
///
/// Enumerated where the choice is made and not kept: a few microseconds once
/// per encoder opened, where a list kept against a card arriving or a display
/// being plugged in would be stale in the one moment it mattered.
#[cfg(windows)]
pub fn machine() -> Vec<Adapter> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1,
    };

    // SAFETY: DXGI is asked for its factory and then for each adapter by index,
    // and every adapter it hands back is released when it goes out of scope.
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut index = 0;
    // Past the last adapter `EnumAdapters1` answers an error, which is the end
    // of the machine's list rather than a failure of this call.
    while let Ok(adapter) = unsafe { factory.EnumAdapters1(index) } {
        index += 1;
        // An adapter whose description can't be read is one no LUID can be
        // matched against, so it is left out rather than ranked on guesses.
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        found.push(Adapter {
            luid: Luid {
                low: desc.AdapterLuid.LowPart,
                high: desc.AdapterLuid.HighPart,
            },
            vendor: desc.VendorId,
            dedicated_memory: desc.DedicatedVideoMemory as u64,
            shared_memory: desc.SharedSystemMemory as u64,
            software: (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0,
        });
    }
    found
}

#[cfg(not(windows))]
pub fn machine() -> Vec<Adapter> {
    Vec::new()
}

static PREFERENCE: AtomicU8 = AtomicU8::new(Preference::Dedicated as u8);

/// Chooses the GPU every stream's device is made on. A stream going on keeps
/// the device it started with, so the choice takes effect at the next one.
pub fn set(preference: Preference) {
    PREFERENCE.store(preference as u8, Ordering::Relaxed);
}

pub fn preference() -> Preference {
    match PREFERENCE.load(Ordering::Relaxed) {
        1 => Preference::Integrated,
        _ => Preference::Dedicated,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AMD, Adapter, Backend, INTEL, Luid, NVIDIA, Preference, Rank, Reach, candidates, choose,
        select,
    };

    const GIB: u64 = 1 << 30;

    /// A card as DXGI lists one.
    fn card(luid: u32, vendor: u32, dedicated: u64) -> Adapter {
        Adapter {
            luid: Luid { low: luid, high: 0 },
            vendor,
            dedicated_memory: dedicated,
            shared_memory: 0,
            software: false,
        }
    }

    /// A GPU on the processor: a nominal share of memory of its own, and up to
    /// half of RAM to draw from.
    fn on_die(luid: u32, vendor: u32) -> Adapter {
        Adapter {
            shared_memory: 8 * GIB,
            ..card(luid, vendor, 128 << 20)
        }
    }

    fn software() -> Adapter {
        Adapter {
            software: true,
            ..card(9, 0x1414, 0)
        }
    }

    fn pick(preference: Preference, adapters: &[Adapter]) -> Option<u32> {
        choose(preference, adapters.iter().copied()).map(|adapter| adapter.luid.low)
    }

    /// The UI's layout.json cut to this one field, which a file written before
    /// the setting has no key for.
    #[derive(serde::Deserialize, Default)]
    #[serde(default)]
    struct Layout {
        gpu: Preference,
    }

    #[test]
    fn dedicated_is_the_default_and_layout_json_says_so_lowercase() {
        assert_eq!(Preference::default(), Preference::Dedicated);
        // The words the file holds, lowercase as `noise_suppression` holds
        // `"rnnoise"`.
        assert_eq!(
            serde_json::to_string(&Preference::Dedicated).unwrap(),
            "\"dedicated\""
        );
        assert_eq!(
            serde_json::to_string(&Preference::Integrated).unwrap(),
            "\"integrated\""
        );
        assert_eq!(
            serde_json::from_str::<Layout>("{\"gpu\":\"integrated\"}")
                .unwrap()
                .gpu,
            Preference::Integrated
        );
        // A file from before the setting is on the card, as it behaved then.
        assert_eq!(
            serde_json::from_str::<Layout>("{}").unwrap().gpu,
            Preference::Dedicated
        );
    }

    #[test]
    fn an_nvidia_card_and_intels_igpu_swap_places_with_the_preference() {
        let adapters = [card(1, NVIDIA, 24 * GIB), on_die(2, INTEL)];
        assert_eq!(
            Preference::Dedicated.rank(&adapters[0]),
            Some(Rank::Preferred)
        );
        assert_eq!(
            Preference::Dedicated.rank(&adapters[1]),
            Some(Rank::Fallback)
        );
        assert_eq!(
            Preference::Integrated.rank(&adapters[1]),
            Some(Rank::Preferred)
        );
        assert_eq!(
            Preference::Integrated.rank(&adapters[0]),
            Some(Rank::Fallback)
        );
        assert_eq!(pick(Preference::Dedicated, &adapters), Some(1));
        assert_eq!(pick(Preference::Integrated, &adapters), Some(2));
    }

    #[test]
    fn amds_card_and_intels_igpu_swap_places_the_same_way() {
        let adapters = [on_die(1, INTEL), card(2, AMD, 16 * GIB)];
        assert_eq!(
            Preference::Dedicated.rank(&adapters[1]),
            Some(Rank::Preferred)
        );
        assert_eq!(
            Preference::Integrated.rank(&adapters[0]),
            Some(Rank::Preferred)
        );
        assert_eq!(pick(Preference::Dedicated, &adapters), Some(2));
        assert_eq!(pick(Preference::Integrated, &adapters), Some(1));
    }

    #[test]
    fn one_gpu_is_chosen_whichever_kind_it_is_and_whichever_is_preferred() {
        let solo_card = [card(1, NVIDIA, 24 * GIB)];
        let solo_igpu = [on_die(1, INTEL)];
        let solo_amd = [card(1, AMD, 16 * GIB)];
        for preference in [Preference::Dedicated, Preference::Integrated] {
            assert_eq!(pick(preference, &solo_card), Some(1), "{preference:?}");
            assert_eq!(pick(preference, &solo_igpu), Some(1), "{preference:?}");
            assert_eq!(pick(preference, &solo_amd), Some(1), "{preference:?}");
        }
        // The one it isn't the preference for has no better rank, so it is
        // what runs, rather than nothing.
        assert_eq!(
            Preference::Integrated.rank(&solo_card[0]),
            Some(Rank::Fallback)
        );
        assert_eq!(
            Preference::Dedicated.rank(&solo_igpu[0]),
            Some(Rank::Fallback)
        );
    }

    #[test]
    fn a_software_adapter_is_never_chosen() {
        let only = [software()];
        for preference in [Preference::Dedicated, Preference::Integrated] {
            assert_eq!(preference.rank(&only[0]), None);
            assert_eq!(pick(preference, &only), None);
        }
        // Not even beside a card it reports more memory than: no encoder runs
        // on the Basic Render Driver.
        let adapters = [software(), card(1, NVIDIA, 24 * GIB)];
        assert_eq!(pick(Preference::Dedicated, &adapters), Some(1));
        assert_eq!(pick(Preference::Integrated, &adapters), Some(1));
    }

    #[test]
    fn an_igpu_reporting_most_of_ram_as_shared_is_still_an_igpu() {
        // The classic misclassification: an Intel UHD 770 offers 128 MiB of
        // dedicated memory and calls half the machine's RAM its own shared
        // memory. Read as "a lot of memory, so a card", a stream would land on
        // the iGPU. The vendor says which kind of GPU it is; memory only sorts
        // adapters already known to be the same kind, so 8 GiB of shared
        // memory changes nothing here.
        let uhd = on_die(2, INTEL);
        assert_eq!(uhd.dedicated_memory, 128 << 20);
        assert_eq!(uhd.shared_memory, 8 * GIB);
        assert_eq!(Preference::Dedicated.rank(&uhd), Some(Rank::Fallback));
        assert_eq!(Preference::Integrated.rank(&uhd), Some(Rank::Preferred));
        let adapters = [uhd, card(1, NVIDIA, 24 * GIB)];
        assert_eq!(pick(Preference::Dedicated, &adapters), Some(1));
    }

    #[test]
    fn of_two_cards_the_one_with_more_memory_of_its_own_wins() {
        let adapters = [card(1, NVIDIA, 8 * GIB), card(2, NVIDIA, 24 * GIB)];
        assert_eq!(pick(Preference::Dedicated, &adapters), Some(2));
        // Whichever order DXGI lists them in.
        let swapped = [adapters[1], adapters[0]];
        assert_eq!(pick(Preference::Dedicated, &swapped), Some(2));
    }

    #[test]
    fn no_adapters_at_all_chooses_nothing() {
        assert_eq!(choose(Preference::Dedicated, Vec::<Adapter>::new()), None);
        assert!(candidates(Preference::Dedicated, &[], None).is_empty());
        // Nor is a machine that lists only the Basic Render Driver a machine
        // with an encoder: it has neither.
        assert!(candidates(Preference::Dedicated, &[software()], None).is_empty());
    }

    /// The backends each platform offers, which is the difference between a
    /// build that can encode on a GPU and one that names an encoder family its
    /// driver doesn't have.
    #[test]
    fn each_platform_offers_the_backends_its_drivers_have() {
        let windows = cfg!(windows);
        assert_eq!(Backend::on(NVIDIA), &[Backend::Nvenc]);
        assert_eq!(
            Backend::on(AMD),
            if windows {
                &[Backend::Amf]
            } else {
                &[Backend::Vaapi]
            }
        );
        assert_eq!(
            Backend::on(INTEL),
            if windows {
                &[Backend::Qsv]
            } else {
                &[Backend::Vaapi]
            }
        );
        // Microsoft's Basic Render Driver, which has no encoder at all.
        assert!(Backend::on(0x1414).is_empty());
    }

    /// The preference's adapter is asked first, and its own encoder's family
    /// with it; the other kind of GPU is a fallback, in the order the
    /// preference and the memory of its own rank them.
    #[test]
    fn the_preferred_adapters_encoder_is_asked_first() {
        let adapters = [card(1, NVIDIA, 24 * GIB), on_die(2, INTEL)];
        let dedicated = candidates(Preference::Dedicated, &adapters, None);
        assert_eq!(dedicated[0].backend, Backend::Nvenc);
        assert_eq!(dedicated[0].adapter, Some(adapters[0]));
        assert_eq!(dedicated[0].rank, Rank::Preferred);
        assert_eq!(dedicated[1].backend, Backend::on(INTEL)[0]);
        assert_eq!(dedicated[1].adapter, Some(adapters[1]));
        assert_eq!(dedicated[1].rank, Rank::Fallback);
        let integrated = candidates(Preference::Integrated, &adapters, None);
        assert_eq!(integrated[0].adapter, Some(adapters[1]));
        assert_eq!(integrated[0].rank, Rank::Preferred);
        assert_eq!(integrated[1].adapter, Some(adapters[0]));
        assert_eq!(integrated[1].rank, Rank::Fallback);
    }

    /// A capture that filled its frame on another adapter than the encoder's is
    /// the copy a fallback costs, and it is said so before either is opened:
    /// the encoder on the capture's own adapter is offered next, the same
    /// adapter and one upload, and the rest of the machine after that.
    #[test]
    fn a_capture_on_the_other_adapter_is_offered_next_and_a_copy_is_reported() {
        let adapters = [
            card(1, NVIDIA, 24 * GIB),
            on_die(2, INTEL),
            card(3, AMD, 16 * GIB),
        ];
        let capture = adapters[2].luid;
        let order = candidates(Preference::Dedicated, &adapters, Some(capture));
        let adapters_asked: Vec<u32> = order
            .iter()
            .map(|candidate| candidate.adapter.unwrap().luid.low)
            .collect();
        assert_eq!(adapters_asked, [1, 3, 2]);
        // The preferred adapter puts the frame across to the encoder; the
        // capture's own adapter only uploads it; the third crosses again.
        assert_eq!(order[0].reach, Reach::Crosses);
        assert_eq!(order[1].reach, Reach::Uploaded);
        assert_eq!(order[2].reach, Reach::Crosses);
        // A frame with no adapter of its own — a camera's, a clip's — has
        // nothing to cross to.
        let in_memory = candidates(Preference::Dedicated, &adapters, None);
        assert!(in_memory.iter().all(|one| one.reach == Reach::Uploaded));
        // And a capture already on the preferred adapter is that same path.
        let on_the_card = candidates(Preference::Dedicated, &adapters, Some(adapters[0].luid));
        assert_eq!(on_the_card[0].reach, Reach::Uploaded);
        assert_eq!(on_the_card[0].rank, Rank::Preferred);
    }

    /// The first candidate that opens is the one that runs, so an encoder that
    /// can't open on the preferred adapter falls to the next one — and the
    /// answer says which adapter it landed on and whether the frame crosses to
    /// get there, rather than calling a fallback the same-adapter path.
    #[test]
    fn the_first_encoder_that_opens_is_the_one_that_runs() {
        let adapters = [card(1, NVIDIA, 24 * GIB), on_die(2, INTEL)];
        // The capture is on the processor's GPU, and the stream asked for it.
        let order = candidates(Preference::Integrated, &adapters, Some(adapters[1].luid));
        assert_eq!(order[0].adapter, Some(adapters[1]));
        // It hasn't got an encoder this build can open — which is this
        // machine, whose FFmpeg has no Quick Sync — so the card's runs, and
        // the frame crosses to it.
        let chosen = select(&order, |candidate| candidate.backend == Backend::Nvenc).unwrap();
        assert_eq!(chosen.adapter, Some(adapters[0]));
        assert_eq!(chosen.rank, Rank::Fallback);
        assert_eq!(chosen.reach, Reach::Crosses);
        // Nothing opens at all: no hardware path, which leaves the software
        // encoder the only one there is.
        assert_eq!(select(&order, |_| false), None);
    }

    /// What this machine's adapters are, and which encoder would be asked
    /// first on each: a probe run by hand, since no machine's adapters are the
    /// test's to decide.
    #[test]
    #[ignore = "asks this machine for its adapters"]
    fn this_machines_adapters_in_the_order_they_are_asked() {
        let adapters = super::machine();
        eprintln!("{adapters:#?}");
        for preference in [Preference::Dedicated, Preference::Integrated] {
            let order = candidates(preference, &adapters, None);
            for candidate in &order {
                eprintln!("{preference:?}: {candidate:?}");
            }
        }
    }
}
