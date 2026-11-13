//! `d3d11.rs`'s device.

use super::*;

/// An encoder NVENC owns, and a source of frames for the tests.
#[repr(C)]
pub(crate) struct Raw {
    pub(crate) _private: [u8; 0],
}

#[repr(C)]
pub(crate) struct RawSource {
    pub(crate) _private: [u8; 0],
}

/// The three shared pictures and the fence they are drawn behind, as
/// native/d3d11enc.c keeps them.
#[repr(C)]
pub(crate) struct RawShared {
    pub(crate) _private: [u8; 0],
}

/// DXGI's vendor id for NVIDIA, whose cards are the only ones carrying NVENC,
/// the only encoder this backend opens. `gpu` ranks adapters by the same number,
/// privately, and this copy is what decides whether the adapter the setting
/// chose can encode here at all.
pub(crate) const NVIDIA: u32 = 0x10DE;

/// An adapter as DXGI describes it: the facts [`gpu::choose`] ranks, and the
/// name its driver gives it, which a failure names it by and [`gpu::Adapter`]
/// has no field for.
pub(crate) struct Named {
    pub(crate) facts: gpu::Adapter,
    pub(crate) name: String,
}

/// The adapters this machine presents, in DXGI's order, which starts with the
/// one the desktop is shown on.
///
/// Enumerated when a device is made and not kept: it costs a few microseconds
/// once per open, where a list kept against a card arriving or a display being
/// plugged in would be stale in the one moment it mattered, and it costs nothing
/// per frame either way.
pub(crate) fn adapters() -> Result<Vec<Named>> {
    // SAFETY: DXGI is asked for its factory and then for each adapter by index,
    // and every adapter it hands back is released when it goes out of scope.
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.map_err(|status| anyhow!("no DXGI factory: {status}"))?;
    let mut found = Vec::new();
    let mut index = 0;
    // Past the last adapter EnumAdapters1 answers DXGI_ERROR_NOT_FOUND, which is
    // the end of the machine's list rather than a failure of this open.
    while let Ok(adapter) = unsafe { factory.EnumAdapters1(index) } {
        index += 1;
        // An adapter whose description can't be read is one no LUID can be
        // matched against, so it is left out rather than ranked on guesses.
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
            continue;
        };
        found.push(Named {
            facts: gpu::Adapter {
                luid: gpu::Luid {
                    low: desc.AdapterLuid.LowPart,
                    high: desc.AdapterLuid.HighPart,
                },
                vendor: desc.VendorId,
                dedicated_memory: desc.DedicatedVideoMemory as u64,
                shared_memory: desc.SharedSystemMemory as u64,
                software: (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0,
            },
            name: wide(&desc.Description),
        });
    }
    Ok(found)
}

/// The adapter this encoder's device is made on, under `preference`: the
/// machine's adapters run through [`gpu::choose`], which is where the policy
/// lives, and the one it answers with handed back by name.
///
/// A chosen adapter that isn't NVIDIA's fails here, before a device or a session
/// is opened. NVENC exists only on NVIDIA's cards, so the GPU built into the
/// processor — what `Integrated` asks for — has no encoder this backend can
/// reach. Saying so in words leaves the caller free to encode another way, where
/// a device quietly made on the other adapter would drop the setting without
/// saying it had.
pub(crate) fn chosen(preference: gpu::Preference) -> Result<Named> {
    let mut machine = adapters()?;
    let Some(best) = gpu::choose(preference, machine.iter().map(|named| named.facts)) else {
        bail!(
            "no hardware adapter to make a device on: this machine has {}",
            listed(&machine)
        );
    };
    // choose answers with one of the adapters it was given, and one machine's
    // adapters have one LUID each, so this finds exactly the one it ranked.
    let index = machine
        .iter()
        .position(|named| named.facts.luid == best.luid)
        .expect("choose answers with an adapter it was given");
    if machine[index].facts.vendor != NVIDIA {
        bail!(
            "no NVENC on {}, the {preference:?} GPU chosen: only NVIDIA's cards carry one, so \
             this backend can't encode there and the stream needs another encoder; this machine \
             has {}",
            machine[index].name,
            listed(&machine)
        );
    }
    Ok(machine.swap_remove(index))
}

/// The machine's adapters as one sentence names them, for a failure that has to
/// say what this machine has rather than only what was wanted.
pub(crate) fn listed(machine: &[Named]) -> String {
    if machine.is_empty() {
        return "no adapters at all".to_owned();
    }
    machine
        .iter()
        .map(|named| format!("{} (0x{:04X})", named.name, named.facts.vendor))
        .collect::<Vec<_>>()
        .join(", ")
}

/// DXGI's description of an adapter as text: UTF-16 with a zero on the end, or a
/// full buffer with none when a driver filled every unit.
pub(crate) fn wide(text: &[u16]) -> String {
    let end = text
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(text.len());
    String::from_utf16_lossy(&text[..end])
}

/// The codec as native/d3d11enc.c numbers it, and the size checked before either
/// open is attempted: NVENC takes even dimensions only, and a codec this backend
/// has no number for is refused in words rather than by a status number.
pub(crate) fn codec_number(codec: Codec, width: u32, height: u32) -> Result<c_uint> {
    if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        bail!("{codec:?} frames need an even size, not {width}x{height}");
    }
    Ok(match codec {
        Codec::H264 => 0,
        Codec::H265 => 1,
        Codec::Av1 => 2,
        Codec::Vp8 => bail!("VP8 doesn't encode on the GPU; libvpx does it on the processor"),
    })
}

/// The text the shim wrote into `bytes`, which ends with a zero.
pub(crate) fn c_text(bytes: &[u8]) -> String {
    std::ffi::CStr::from_bytes_until_nul(bytes).map_or_else(
        |_| String::new(),
        |text| text.to_string_lossy().into_owned(),
    )
}
