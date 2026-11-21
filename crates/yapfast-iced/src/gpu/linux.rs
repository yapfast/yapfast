//! Importing a stream worker's DMA-BUF picture where it lies, without a copy.
//!
//! The worker draws into a GBM buffer and sends the picture as DMA-BUF
//! descriptors over the stream's socket. This module imports those descriptors
//! on the window's Vulkan device as external memory — one `VkImage` with each
//! plane's offset, stride and the DRM modifier the exporter named, and one
//! `VkDeviceMemory` per buffer the planes are in — and hands the texture to
//! iced_wgpu's image pipeline, which samples it over its whole 0..1 UV.
//!
//! A picture Skia can sample as it is (RGBA or BGRA) is bound directly. The
//! card's decoder exports NV12 (and 10-bit P010/P016), whose two planes the
//! image shader can't read, so those go through the tile pass first: one
//! triangle that samples the luma and chroma planes and writes an RGBA tile the
//! image shader samples like any other picture. A picture this build can't
//! import is refused once, and the stream then sends pixels.

use std::cell::RefCell;
use std::os::fd::{FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::sync::Arc;

use ash::{khr, vk};

use super::{dmabuf, wgpu};

const MAX_PLANES: usize = dmabuf::MAX_PLANES;

/// `DRM_FORMAT_MOD_INVALID`: the exporter didn't say how the memory is laid out,
/// and a tiling nobody named can't be imported as any of them.
const MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;

/// The fourcc's own packing: the four characters, the first in the low byte.
const fn fourcc(name: [u8; 4]) -> u32 {
    (name[0] as u32) | ((name[1] as u32) << 8) | ((name[2] as u32) << 16) | ((name[3] as u32) << 24)
}

/// What a picture's memory holds, as far as this importer can show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Rgba,
    Bgra,
    Nv12,
    P010,
    P016,
}

impl Shape {
    const ABGR8888: u32 = fourcc(*b"AB24");
    const ARGB8888: u32 = fourcc(*b"AR24");
    const NV12: u32 = fourcc(*b"NV12");
    const P010_FOURCC: u32 = fourcc(*b"P010");
    const P016_FOURCC: u32 = fourcc(*b"P016");

    fn of(fourcc: u32) -> Result<Self, String> {
        match fourcc {
            Self::ABGR8888 => Ok(Self::Rgba),
            Self::ARGB8888 => Ok(Self::Bgra),
            Self::NV12 => Ok(Self::Nv12),
            Self::P010_FOURCC => Ok(Self::P010),
            Self::P016_FOURCC => Ok(Self::P016),
            other => Err(format!(
                "the picture is DRM format {other:08x} ({}), which this importer can't show",
                spelled(other)
            )),
        }
    }

    fn named(self) -> &'static str {
        match self {
            Self::Rgba => "RGBA",
            Self::Bgra => "BGRA",
            Self::Nv12 => "NV12",
            Self::P010 => "P010",
            Self::P016 => "P016",
        }
    }

    fn planes(self) -> usize {
        match self {
            Self::Rgba | Self::Bgra => 1,
            Self::Nv12 | Self::P010 | Self::P016 => 2,
        }
    }

    /// Whether the shape is one the tile pass reads rather than one Skia samples
    /// as it is.
    fn tiled(self) -> bool {
        self.planes() == 2
    }

    fn subsampled(self) -> bool {
        self.tiled()
    }

    /// The fewest bytes one row of `plane` takes in a picture `width` pixels wide.
    fn row(self, width: u32, _plane: usize) -> u32 {
        match self {
            Self::Rgba | Self::Bgra => width.saturating_mul(4),
            Self::Nv12 => width,
            Self::P010 | Self::P016 => width.saturating_mul(2),
        }
    }

    /// Where in a 16-bit word a 10-bit sample's bits are: 0 for 8-bit shapes.
    fn depth(self) -> u32 {
        match self {
            Self::Rgba | Self::Bgra | Self::Nv12 => 0,
            Self::P010 => 1,
            Self::P016 => 2,
        }
    }

    fn vk(self) -> vk::Format {
        match self {
            Self::Rgba => vk::Format::R8G8B8A8_UNORM,
            Self::Bgra => vk::Format::B8G8R8A8_UNORM,
            Self::Nv12 => vk::Format::G8_B8R8_2PLANE_420_UNORM,
            Self::P010 => vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
            Self::P016 => vk::Format::G16_B16R16_2PLANE_420_UNORM,
        }
    }

    fn wgpu(self) -> wgpu::TextureFormat {
        match self {
            Self::Rgba => wgpu::TextureFormat::Rgba8Unorm,
            Self::Bgra => wgpu::TextureFormat::Bgra8Unorm,
            Self::Nv12 => wgpu::TextureFormat::NV12,
            Self::P010 | Self::P016 => wgpu::TextureFormat::P010,
        }
    }

    fn view(self, plane: usize) -> (wgpu::TextureFormat, wgpu::TextureAspect) {
        match (self, plane) {
            (Self::Nv12, 0) => (wgpu::TextureFormat::R8Unorm, wgpu::TextureAspect::Plane0),
            (Self::Nv12, _) => (wgpu::TextureFormat::Rg8Unorm, wgpu::TextureAspect::Plane1),
            (Self::P010 | Self::P016, 0) => {
                (wgpu::TextureFormat::R16Unorm, wgpu::TextureAspect::Plane0)
            }
            (Self::P010 | Self::P016, _) => {
                (wgpu::TextureFormat::Rg16Unorm, wgpu::TextureAspect::Plane1)
            }
            (Self::Rgba | Self::Bgra, 0) => (self.wgpu(), wgpu::TextureAspect::All),
            (Self::Rgba | Self::Bgra, _) => (self.wgpu(), wgpu::TextureAspect::All),
        }
    }

    fn uses(self) -> wgpu::TextureUsages {
        match self {
            Self::Rgba | Self::Bgra => {
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT
            }
            Self::Nv12 | Self::P010 | Self::P016 => wgpu::TextureUsages::TEXTURE_BINDING,
        }
    }
}

/// The fourcc's own characters, for a log line.
fn spelled(fourcc: u32) -> String {
    fourcc
        .to_le_bytes()
        .iter()
        .map(|byte| match byte {
            0x20..=0x7e => char::from(*byte),
            other => char::from(*other).escape_debug().next().unwrap_or('?'),
        })
        .collect()
}

/// One plane, placed in the memory its descriptor names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Placed {
    offset: u32,
    stride: u32,
    /// Which of [`Plan::memories`] holds this plane.
    memory: usize,
}

/// What the importer is to make of one picture.
#[derive(Debug)]
struct Plan {
    width: u32,
    height: u32,
    modifier: u64,
    shape: Shape,
    colour: dmabuf::Colour,
    planes: [Option<Placed>; MAX_PLANES],
    memories: [Option<RawFd>; MAX_PLANES],
}

impl Plan {
    fn memories(&self) -> impl Iterator<Item = RawFd> + '_ {
        self.memories.iter().flatten().copied()
    }

    fn disjoint(&self) -> bool {
        self.memories().nth(1).is_some()
    }
}

/// A picture's numbers, as the importer reads them.
struct Numbers {
    width: u32,
    height: u32,
    fourcc: u32,
    modifier: u64,
    colour: dmabuf::Colour,
    planes: [Option<(dmabuf::Plane, RawFd)>; MAX_PLANES],
}

impl Numbers {
    fn of(picture: &dmabuf::Picture) -> Self {
        let mut planes = [None; MAX_PLANES];
        for (index, plane) in picture.planes().enumerate() {
            planes[index] = Some(plane);
        }
        Self {
            width: picture.width(),
            height: picture.height(),
            fourcc: picture.fourcc(),
            modifier: picture.modifier(),
            colour: picture.colour(),
            planes,
        }
    }
}

/// What importing `numbers` takes, or why the picture can't be imported.
fn layout(numbers: &Numbers) -> Result<Plan, String> {
    let Numbers {
        width,
        height,
        fourcc,
        modifier,
        colour,
        planes,
    } = *numbers;
    if width == 0 || height == 0 {
        return Err(format!("the picture is {width} by {height}"));
    }
    let shape = Shape::of(fourcc)?;
    if shape.subsampled() && (!width.is_multiple_of(2) || !height.is_multiple_of(2)) {
        return Err(format!(
            "a {} picture of {width} by {height} has half a chroma sample at its edge",
            shape.named()
        ));
    }
    if modifier == MOD_INVALID {
        return Err(
            "the picture was exported without saying how its memory is laid out".to_owned(),
        );
    }
    let described: Vec<(dmabuf::Plane, RawFd)> = planes.iter().flatten().copied().collect();
    if described.len() != shape.planes() {
        return Err(format!(
            "a {} picture has {} plane(s), and this one came with {}",
            shape.named(),
            shape.planes(),
            described.len()
        ));
    }
    let mut placed: [Option<Placed>; MAX_PLANES] = [None; MAX_PLANES];
    let mut memories: [Option<RawFd>; MAX_PLANES] = [None; MAX_PLANES];
    let mut buffers = 0;
    for (index, (plane, fd)) in described.iter().enumerate() {
        let least = shape.row(width, index);
        if plane.stride < least {
            return Err(format!(
                "plane {index} of the {} picture has rows of {} bytes, and {width} pixels take at least {least}",
                shape.named(),
                plane.stride
            ));
        }
        let memory = match memories.iter().flatten().position(|held| held == fd) {
            Some(memory) => memory,
            None => {
                let memory = buffers;
                memories[memory] = Some(*fd);
                buffers += 1;
                memory
            }
        };
        placed[index] = Some(Placed {
            offset: plane.offset,
            stride: plane.stride,
            memory,
        });
    }
    Ok(Plan {
        width,
        height,
        modifier,
        shape,
        colour,
        planes: placed,
        memories,
    })
}

/// Imports a picture on `device` and binds it as the image pipeline's texture,
/// or returns `None` if this build can't (the stream then sends pixels).
pub(super) fn import(
    device: &wgpu::Device,
    layout_bind: &wgpu::BindGroupLayout,
    picture: &dmabuf::Picture,
) -> Option<Arc<wgpu::BindGroup>> {
    let plan = match layout(&Numbers::of(picture)) {
        Ok(plan) => plan,
        Err(error) => {
            tracing::debug!(%error, "a stream's DMA-BUF picture isn't one this build can show");
            return None;
        }
    };
    let texture = match unsafe { texture(device, &plan) } {
        Ok(texture) => texture,
        Err(error) => {
            tracing::debug!(%error, "could not import a stream's DMA-BUF picture");
            return None;
        }
    };
    // A picture Skia reads as it is, or one the tile pass turns into one.
    let sampled = if plan.shape.tiled() {
        let queue = super::queue()?;
        match tile(device, queue, &texture, &plan) {
            Ok(tile) => tile,
            Err(error) => {
                tracing::debug!(%error, "could not read a stream's DMA-BUF picture");
                return None;
            }
        }
    } else {
        texture
    };
    let view = sampled.create_view(&wgpu::TextureViewDescriptor {
        label: Some("a stream's DMA-BUF picture"),
        format: None,
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        usage: Some(wgpu::TextureUsages::TEXTURE_BINDING),
        aspect: wgpu::TextureAspect::All,
        base_mip_level: 0,
        mip_level_count: None,
        base_array_layer: 0,
        array_layer_count: Some(1),
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("a stream's DMA-BUF picture"),
        layout: layout_bind,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&view),
        }],
    });
    Some(Arc::new(bind_group))
}

/// A texture over the memory a picture's descriptors name, made on the window's
/// device through Vulkan's external memory: no pixel of the picture is copied.
///
/// # Safety
///
/// The plan must be what the picture says, and the picture must have been taken
/// from its stream's socket, so that the descriptors in it name the memory the
/// worker shared.
unsafe fn texture(device: &wgpu::Device, plan: &Plan) -> Result<wgpu::Texture, String> {
    // wgpu 27 has no `Features` bit for DMA-BUF: wgpu-hal enables
    // `VK_EXT_external_memory_dma_buf` whenever the driver supports it, and a
    // device that doesn't answers the import below with an error.
    if plan.shape == Shape::Nv12
        && !device
            .features()
            .contains(wgpu::Features::TEXTURE_FORMAT_NV12)
    {
        return Err("the window's Vulkan device has no NV12 texture".to_owned());
    }
    if plan.shape.tiled()
        && plan.shape.depth() != 0
        && !device
            .features()
            .contains(wgpu::Features::TEXTURE_FORMAT_P010)
    {
        return Err(
            "the window's Vulkan device has no 16-bit texture for a 10-bit picture".to_owned(),
        );
    }
    // SAFETY: the device is the window's and is alive here.
    let hal = unsafe { device.as_hal::<wgpu::hal::vulkan::Api>() }
        .ok_or("the window's device is not Vulkan")?;
    let raw = hal.raw_device();
    let instance = hal.shared_instance().raw_instance();
    let physical = hal.raw_physical_device();
    let fd_fn = khr::external_memory_fd::Device::new(hal.shared_instance().raw_instance(), raw);

    let layouts: Vec<vk::SubresourceLayout> = plan
        .planes
        .iter()
        .flatten()
        .map(|placed| vk::SubresourceLayout {
            offset: u64::from(placed.offset),
            row_pitch: u64::from(placed.stride),
            size: 0,
            array_pitch: 0,
            depth_pitch: 0,
        })
        .collect();
    let mut external = vk::ExternalMemoryImageCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let mut modifier = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
        .drm_format_modifier(plan.modifier)
        .plane_layouts(&layouts);
    let mut flags = vk::ImageCreateFlags::empty();
    if plan.disjoint() {
        flags |= vk::ImageCreateFlags::DISJOINT;
    }
    let info = vk::ImageCreateInfo::default()
        .flags(flags)
        .image_type(vk::ImageType::TYPE_2D)
        .format(plan.shape.vk())
        .extent(vk::Extent3D {
            width: plan.width,
            height: plan.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(
            if plan
                .shape
                .uses()
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            {
                vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::COLOR_ATTACHMENT
            } else {
                vk::ImageUsageFlags::SAMPLED
            },
        )
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    let info = info.push_next(&mut external);
    let info = info.push_next(&mut modifier);
    // SAFETY: the image is made on the device the plan is for, and the two
    // structures it chains are alive for the call.
    let image = unsafe { raw.create_image(&info, None) }
        .map_err(|error| format!("Vulkan wouldn't make the picture's image: {error}"))?;
    let mut fresh = Fresh {
        device: raw.handle(),
        destroy_image: raw.fp_v1_0().destroy_image,
        free_memory: raw.fp_v1_0().free_memory,
        image,
        memories: [None; MAX_PLANES],
        handed_over: false,
    };

    for (index, fd) in plan.memories().enumerate() {
        let fd = duplicate(fd)?;
        let requirements = if plan.disjoint() {
            let mut plane_info =
                vk::ImagePlaneMemoryRequirementsInfo::default().plane_aspect(plane_aspect(index));
            let asked = vk::ImageMemoryRequirementsInfo2::default()
                .image(image)
                .push_next(&mut plane_info);
            let mut answer = vk::MemoryRequirements2::default();
            // SAFETY: the image is this one and the structure says which plane.
            unsafe { raw.get_image_memory_requirements2(&asked, &mut answer) };
            answer.memory_requirements
        } else {
            // SAFETY: the image is this one, alive, and made on this device.
            unsafe { raw.get_image_memory_requirements(image) }
        };
        fresh.memories[index] = Some(import_memory(
            raw,
            instance,
            physical,
            &fd_fn,
            fd,
            image,
            &requirements,
        )?);
    }

    if plan.disjoint() {
        for (index, memory) in fresh.memories.iter().flatten().enumerate() {
            let mut plane_info =
                vk::BindImagePlaneMemoryInfo::default().plane_aspect(plane_aspect(index));
            let bind = vk::BindImageMemoryInfo::default()
                .image(image)
                .memory(*memory)
                .push_next(&mut plane_info);
            // SAFETY: the memory was imported for this plane of this image.
            unsafe { raw.bind_image_memory2(&[bind]) }.map_err(|error| {
                format!("Vulkan wouldn't bind a picture's plane memory: {error}")
            })?;
        }
    } else if let Some(memory) = fresh.memories[0] {
        // SAFETY: the memory was imported for this image, whose planes are all
        // in that one buffer.
        unsafe { raw.bind_image_memory(image, memory, 0) }
            .map_err(|error| format!("Vulkan wouldn't bind a picture's memory: {error}"))?;
    }

    let hal_desc = wgpu::hal::TextureDescriptor {
        label: Some("a stream's DMA-BUF picture"),
        size: wgpu::Extent3d {
            width: plan.width,
            height: plan.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: plan.shape.wgpu(),
        usage: if plan
            .shape
            .uses()
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        {
            wgpu::wgt::TextureUses::RESOURCE | wgpu::wgt::TextureUses::COLOR_TARGET
        } else {
            wgpu::wgt::TextureUses::RESOURCE
        },
        memory_flags: wgpu::hal::MemoryFlags::empty(),
        view_formats: Vec::new(),
    };
    let memories = fresh.memories;
    let (device_handle, destroy_image, free_memory) =
        (fresh.device, fresh.destroy_image, fresh.free_memory);
    // SAFETY: the image was made on this device and owns the memory bound to it;
    // the callback frees both when the last texture over it is gone.
    let hal_texture = unsafe {
        hal.texture_from_raw(
            image,
            &hal_desc,
            Some(Box::new(move || {
                // SAFETY: the device outlives a texture it made, and nothing
                // else frees this memory or destroys this image. The block this
                // closure is written in is unsafe, so it needs none of its own.
                for memory in memories.iter().flatten() {
                    (free_memory)(device_handle, *memory, std::ptr::null());
                }
                (destroy_image)(device_handle, image, std::ptr::null());
            })),
        )
    };
    fresh.handed_over = true;
    let desc = wgpu::TextureDescriptor {
        label: Some("a stream's DMA-BUF picture"),
        size: hal_desc.size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: plan.shape.wgpu(),
        usage: plan.shape.uses(),
        view_formats: &[],
    };
    // SAFETY: the hal texture was made from this device's handle, with the
    // description above, and the memory behind it holds the picture.
    Ok(unsafe { device.create_texture_from_hal::<wgpu::hal::vulkan::Api>(hal_texture, &desc) })
}

/// The image and the memories bound to it, until they are wgpu's.
struct Fresh {
    device: vk::Device,
    destroy_image: vk::PFN_vkDestroyImage,
    free_memory: vk::PFN_vkFreeMemory,
    image: vk::Image,
    memories: [Option<vk::DeviceMemory>; MAX_PLANES],
    handed_over: bool,
}

impl Drop for Fresh {
    fn drop(&mut self) {
        if self.handed_over {
            return;
        }
        // SAFETY: nothing else was given this image or these memories.
        unsafe {
            for memory in self.memories.iter().flatten() {
                (self.free_memory)(self.device, *memory, std::ptr::null());
            }
            (self.destroy_image)(self.device, self.image, std::ptr::null());
        }
    }
}

/// Imports one buffer's memory from a descriptor this process owns.
fn import_memory(
    raw: &ash::Device,
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    fd_fn: &khr::external_memory_fd::Device,
    fd: OwnedFd,
    image: vk::Image,
    requirements: &vk::MemoryRequirements,
) -> Result<vk::DeviceMemory, String> {
    let fd = fd.into_raw_fd();
    let mut properties = vk::MemoryFdPropertiesKHR::default();
    // SAFETY: the descriptor is one this process holds, of DMA-BUF memory.
    if let Err(error) = unsafe {
        fd_fn.get_memory_fd_properties(
            vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
            fd,
            &mut properties,
        )
    } {
        // SAFETY: Vulkan didn't consume the descriptor, so it is still ours.
        unsafe { libc::close(fd) };
        return Err(format!(
            "Vulkan wouldn't read a picture's descriptor: {error}"
        ));
    }
    let Some(memory_type) = memory_type(
        instance,
        physical,
        requirements.memory_type_bits & properties.memory_type_bits,
    ) else {
        // SAFETY: nothing consumed the descriptor.
        unsafe { libc::close(fd) };
        return Err("no memory type on the window's device takes the picture's buffer".to_owned());
    };
    let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
    let mut imported = vk::ImportMemoryFdInfoKHR::default()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
        .fd(fd);
    let allocate = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type)
        .push_next(&mut imported)
        .push_next(&mut dedicated);
    // SAFETY: the descriptor is a DMA-BUF's, the size and type are the image's,
    // and a picture's memory is dedicated to the image it is imported for.
    match unsafe { raw.allocate_memory(&allocate, None) } {
        Ok(memory) => Ok(memory),
        Err(error) => {
            // SAFETY: the allocation failed, so the descriptor is still ours.
            unsafe { libc::close(fd) };
            Err(format!(
                "Vulkan wouldn't import a picture's memory: {error}"
            ))
        }
    }
}

/// A memory type the image can be bound to that the buffer's own device memory
/// is: the first the two agree on, preferring the one the card keeps to itself.
fn memory_type(instance: &ash::Instance, physical: vk::PhysicalDevice, bits: u32) -> Option<u32> {
    // SAFETY: the physical device is the one the window's device was made on.
    let properties = unsafe { instance.get_physical_device_memory_properties(physical) };
    for (index, kind) in properties.memory_types_as_slice().iter().enumerate() {
        let bit = 1u32 << index;
        if bits & bit != 0
            && kind
                .property_flags
                .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
        {
            return Some(index as u32);
        }
    }
    (0..32).find(|index| bits & (1 << index) != 0)
}

/// A descriptor this process owns for the same memory `fd` names:
/// `F_DUPFD_CLOEXEC`, so the copy is nobody's but this import's.
fn duplicate(fd: RawFd) -> Result<OwnedFd, String> {
    // SAFETY: the number is one the picture holds, and a duplicate is what this
    // asks the kernel for.
    let duplicated = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
    if duplicated < 0 {
        return Err(format!(
            "could not duplicate a picture's descriptor: {}",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: the number was just made for this process, and nothing else holds it.
    Ok(unsafe { OwnedFd::from_raw_fd(duplicated) })
}

/// Which plane of a disjoint image index `index` names.
fn plane_aspect(index: usize) -> vk::ImageAspectFlags {
    match index {
        0 => vk::ImageAspectFlags::PLANE_0,
        1 => vk::ImageAspectFlags::PLANE_1,
        _ => vk::ImageAspectFlags::PLANE_2,
    }
}

/// The format the tile the pass writes is in: the same BGRA the image shader
/// samples.
const TILE: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

thread_local! {
    /// The pass that turns a picture's planes into a tile, made once for the
    /// window's thread: a pipeline is a shader compile, and one is enough.
    static PASS: RefCell<Option<Pass>> = const { RefCell::new(None) };
}

/// Reads the picture's two planes into a tile the picture's own size, on the
/// window's queue, and says the texture it wrote.
fn tile(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    picture: &wgpu::Texture,
    plan: &Plan,
) -> Result<wgpu::Texture, String> {
    PASS.with_borrow_mut(|pass| {
        if pass.is_none() {
            *pass = Some(Pass::new(device)?);
        }
        pass.as_ref()
            .expect("the pass was just made")
            .tile(device, queue, picture, plan)
    })
}

/// The pass that turns the card's picture into what the image shader samples:
/// one triangle over the tile, reading the picture's luma and chroma planes and
/// writing colour.
struct Pass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// The colour signal the shader reads, in one uniform.
    signal: wgpu::Buffer,
    written: RefCell<Option<[u32; 4]>>,
}

impl Pass {
    fn new(device: &wgpu::Device) -> Result<Self, String> {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("a stream's picture planes"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty,
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("a stream's picture planes"),
            entries: &[
                entry(
                    0,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    1,
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                ),
                entry(
                    2,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                entry(
                    3,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                ),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("a stream's picture tile"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("a stream's picture tile"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: TILE,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("a stream's picture planes"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // Four numbers, the uniform block's own size.
        let signal = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("a stream's picture colour"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            pipeline,
            layout,
            sampler,
            signal,
            written: RefCell::new(None),
        })
    }

    fn tile(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        picture: &wgpu::Texture,
        plan: &Plan,
    ) -> Result<wgpu::Texture, String> {
        let (width, height, shape, colour) = (plan.width, plan.height, plan.shape, plan.colour);
        let plane = |label, plane| {
            let (format, aspect) = shape.view(plane);
            picture.create_view(&wgpu::TextureViewDescriptor {
                label: Some(label),
                format: Some(format),
                dimension: Some(wgpu::TextureViewDimension::D2),
                usage: None,
                aspect,
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            })
        };
        let luma = plane("a stream's picture luma", 0);
        let chroma = plane("a stream's picture chroma", 1);
        let signal = [
            colour.range as u32,
            colour.matrix as u32,
            colour.transfer as u32,
            shape.depth(),
        ];
        if *self.written.borrow() != Some(signal) {
            let mut bytes = [0; 16];
            for (word, number) in bytes.chunks_exact_mut(4).zip(signal) {
                word.copy_from_slice(&number.to_ne_bytes());
            }
            queue.write_buffer(&self.signal, 0, &bytes);
            *self.written.borrow_mut() = Some(signal);
        }
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("a stream's picture planes"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&luma),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&chroma),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.signal.as_entire_binding(),
                },
            ],
        });
        let tile = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("a stream's picture tile"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TILE,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let target = tile.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("a stream's picture tile"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("a stream's picture tile"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit(Some(encoder.finish()));
        Ok(tile)
    }
}

/// One triangle over the whole tile: its vertices are named by their index, so
/// it needs no buffer, and its texture coordinates come out of the same numbers.
/// The 8-bit path converts BT.709 limited-range NV12; a 10-bit picture is one of
/// two matrices, a range and a curve, which the uniform carries in.
const SHADER: &str = r#"
struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> Vertex {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: Vertex;
    out.position = vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@group(0) @binding(0) var luma: texture_2d<f32>;
@group(0) @binding(1) var chroma: texture_2d<f32>;
@group(0) @binding(2) var planes: sampler;

// x = range (0 limited, 1 full), y = matrix (0 BT.709, 1 BT.2020 NCL),
// z = curve (0 SDR, 1 PQ), w = depth (0 eight bits, 1 P010, 2 P016).
@group(0) @binding(3) var<uniform> signal: vec4<u32>;

const LUMA_LIMITED = vec2<f32>(64.0, 940.0);
const CHROMA_LIMITED = vec2<f32>(64.0, 960.0);

fn ten(bit: f32, depth: u32) -> f32 {
    let word = bit * 65535.0;
    var code = word / 64.0;
    if depth == 2u {
        code = word - 64.0 * floor(word / 64.0);
    }
    return clamp(code, 0.0, 1023.0);
}

fn expand(code: f32, limited: vec2<f32>, full: bool) -> f32 {
    if full {
        return code / 1023.0;
    }
    return (code - limited.x) / (limited.y - limited.x);
}

fn from_pq(e: f32) -> f32 {
    let m1 = 2610.0 / 16384.0;
    let m2 = (2523.0 / 4096.0) * 128.0;
    let c1 = 3424.0 / 4096.0;
    let c2 = (2413.0 / 4096.0) * 32.0;
    let c3 = (2392.0 / 4096.0) * 32.0;
    let p = pow(max(e, 0.0), 1.0 / m2);
    return pow(max(p - c1, 0.0) / max(c2 - c3 * p, 1e-5), 1.0 / m1);
}

fn tone_map(light: vec3<f32>) -> vec3<f32> {
    let luma = dot(light, vec3<f32>(0.2627, 0.6780, 0.0593));
    if luma <= 0.0 {
        return light;
    }
    let knee = 0.85;
    let peak = 4.0;
    var mapped = luma;
    if luma > knee {
        let t = min((luma - knee) / (peak - knee), 1.0);
        mapped = knee + (1.0 - knee) * (t * t * (3.0 - 2.0 * t));
    }
    return light * (mapped / luma);
}

fn to_bt709(light: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        1.6605 * light.x - 0.5876 * light.y - 0.0729 * light.z,
        -0.1246 * light.x + 1.1329 * light.y - 0.0083 * light.z,
        -0.0182 * light.x - 0.1006 * light.y + 1.1187 * light.z,
    );
}

fn encoded(light: vec3<f32>) -> vec3<f32> {
    return pow(clamp(light, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / 2.4));
}

@fragment
fn fs_main(in: Vertex) -> @location(0) vec4<f32> {
    let luma_bit = textureSample(luma, planes, in.uv).r;
    let chroma_bit = textureSample(chroma, planes, in.uv).rg;
    if signal.w == 0u {
        let y = 1.164 * (luma_bit - 16.0 / 255.0);
        let c = chroma_bit - vec2<f32>(128.0 / 255.0);
        let rgb = vec3<f32>(
            y + 1.793 * c.y,
            y - 0.213 * c.x - 0.533 * c.y,
            y + 2.112 * c.x,
        );
        return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
    }
    let full = signal.x == 1u;
    let depth = signal.w;
    let y = expand(ten(luma_bit, depth), LUMA_LIMITED, full);
    let c = vec2<f32>(
        expand(ten(chroma_bit.r, depth), CHROMA_LIMITED, full) - 0.5,
        expand(ten(chroma_bit.g, depth), CHROMA_LIMITED, full) - 0.5,
    );
    var rgb: vec3<f32>;
    if signal.y == 1u {
        rgb = vec3<f32>(
            y + 1.4746 * c.y,
            y - 0.16455 * c.x - 0.57135 * c.y,
            y + 1.8814 * c.x,
        );
    } else {
        rgb = vec3<f32>(
            y + 1.5748 * c.y,
            y - 0.1873 * c.x - 0.4681 * c.y,
            y + 1.8556 * c.x,
        );
    }
    if signal.z == 1u {
        let light = vec3<f32>(from_pq(rgb.x), from_pq(rgb.y), from_pq(rgb.z)) * 100.0;
        return vec4<f32>(encoded(to_bt709(tone_map(light))), 1.0);
    }
    let light = pow(max(rgb, vec3<f32>(0.0)), vec3<f32>(2.4));
    return vec4<f32>(encoded(to_bt709(light)), 1.0);
}
"#;
