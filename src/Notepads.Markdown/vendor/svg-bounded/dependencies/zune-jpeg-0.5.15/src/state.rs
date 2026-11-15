/*
 * Copyright (c) 2023.
 * This software is redistributable under the MIT, Apache License or Zlib license.
 * Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
 */
use crate::components::SampleRatios;
use crate::decoder::{GainMapInfo, ImageInfo};
use crate::misc::SOFMarkers;
use crate::storage::{Buffer, Control};
use alloc::vec::Vec;

pub(crate) struct GainMapState {
    pub data: Buffer<u8>,
}
pub(crate) struct ImageState {
    /// Width of the image
    pub width: u16,
    /// Height of image
    pub height: u16,
    /// PixelDensity
    pub pixel_density: u8,
    /// Start of frame markers
    pub sof: SOFMarkers,
    /// Horizontal sample
    pub x_density: u16,
    /// Vertical sample
    pub y_density: u16,
    /// Number of components
    pub components: u8,
    /// Gain Map information, useful for
    /// UHDR images
    pub gain_map_info: Buffer<GainMapState>,
    /// Multi picture information, useful for
    /// UHDR images
    pub multi_picture_information: Option<Buffer<u8>>,
    /// Exif Data
    pub exif_data: Option<Buffer<u8>>,
    /// XMP Data
    pub xmp_data: Option<Buffer<u8>>,
    /// IPTC Data
    pub iptc_data: Option<Buffer<u8>>,
    /// Extended XMP Data
    pub extended_xmp: Option<Buffer<u8>>,
    /// Extended XMP Guid
    pub extended_xmp_guid: Option<Buffer<u8>>,
    /// Image sub-sampling ratio
    pub sample_ratio: SampleRatios,
    /// The offset at which Multi picture information was found
    pub multi_picture_information_offset: Option<u64>,
}

impl ImageState {
    pub(crate) fn new(control: &Control) -> Self {
        Self {
            width: 0,
            height: 0,
            pixel_density: 0,
            sof: SOFMarkers::default(),
            x_density: 0,
            y_density: 0,
            components: 0,
            gain_map_info: Buffer::new(control),
            multi_picture_information: None,
            exif_data: None,
            xmp_data: None,
            iptc_data: None,
            extended_xmp: None,
            extended_xmp_guid: None,
            sample_ratio: SampleRatios::default(),
            multi_picture_information_offset: None,
        }
    }
    pub(crate) fn to_plain(&self) -> Option<ImageInfo> {
        fn blob(input: &Option<Buffer<u8>>) -> Option<Option<Vec<u8>>> {
            match input {
                None => Some(None),
                Some(value) => Some(Some(value.as_plain_vec()?.clone())),
            }
        }
        let mut gain_map_info = Vec::new();
        for value in self.gain_map_info.as_plain_vec()? {
            gain_map_info.push(GainMapInfo {
                data: value.data.as_plain_vec()?.clone(),
            });
        }
        Some(ImageInfo {
            width: self.width,
            height: self.height,
            pixel_density: self.pixel_density,
            sof: self.sof,
            x_density: self.x_density,
            y_density: self.y_density,
            components: self.components,
            gain_map_info,
            multi_picture_information: blob(&self.multi_picture_information)?,
            exif_data: blob(&self.exif_data)?,
            xmp_data: blob(&self.xmp_data)?,
            iptc_data: blob(&self.iptc_data)?,
            extended_xmp: blob(&self.extended_xmp)?,
            extended_xmp_guid: blob(&self.extended_xmp_guid)?,
            sample_ratio: self.sample_ratio,
            multi_picture_information_offset: self.multi_picture_information_offset,
        })
    }
}
impl ImageState {
    /// Set width of the image
    ///
    /// Found in the start of frame

    pub(crate) fn set_width(&mut self, width: u16) {
        self.width = width;
    }

    /// Set height of the image
    ///
    /// Found in the start of frame

    pub(crate) fn set_height(&mut self, height: u16) {
        self.height = height;
    }

    /// Set the image density
    ///
    /// Found in the start of frame

    pub(crate) fn set_density(&mut self, density: u8) {
        self.pixel_density = density;
    }

    /// Set image Start of frame marker
    ///
    /// found in the Start of frame header

    pub(crate) fn set_sof_marker(&mut self, marker: SOFMarkers) {
        self.sof = marker;
    }
}
