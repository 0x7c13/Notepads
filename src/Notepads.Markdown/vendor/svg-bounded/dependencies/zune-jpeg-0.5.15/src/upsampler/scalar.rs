/*
 * Copyright (c) 2023.
 *
 * This software is free software;
 *
 * You can redistribute it or modify it under terms of the MIT, Apache License or Zlib license
 * Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
 */
use crate::errors::DecodeErrors;
use crate::storage::{self, Control};

pub fn upsample_horizontal_controlled(
    input: &[i16],
    _ref: &[i16],
    _in_near: &[i16],
    _scratch: &mut [i16],
    output: &mut [i16],
    control: &Control,
) -> Result<(), DecodeErrors> {
    storage::step(control, 0)?;
    assert_eq!(
        input.len() * 2,
        output.len(),
        "Input length is not half the size of the output length"
    );
    assert!(
        output.len() > 4 && input.len() > 2,
        "Too Short of a vector, cannot upsample"
    );

    storage::fixed(control, 2)?;
    output[0] = input[0];
    output[1] = (input[0] * 3 + input[1] + 2) >> 2;

    for __ms_work_0 in crate::storage::polled(
        output[2..].chunks_exact_mut(2).zip(input.windows(3)),
        control,
        2,
    ) {
        let (output_window, input_window) = __ms_work_0?;
        let sample = 3 * input_window[1] + 2;

        output_window[0] = (sample + input_window[0]) >> 2;
        output_window[1] = (sample + input_window[2]) >> 2;
    }
    // Get lengths
    let out_len = output.len() - 2;
    let input_len = input.len() - 2;

    // slice the output vector
    let f_out = &mut output[out_len..];
    let i_last = &input[input_len..];

    // write out manually..
    storage::fixed(control, 2)?;
    f_out[0] = (3 * i_last[1] + i_last[0] + 2) >> 2;
    f_out[1] = i_last[1];

    Ok(())
}
pub fn upsample_vertical_controlled(
    input: &[i16],
    in_near: &[i16],
    in_far: &[i16],
    _scratch_space: &mut [i16],
    output: &mut [i16],
    control: &Control,
) -> Result<(), DecodeErrors> {
    storage::step(control, 0)?;
    assert_eq!(input.len() * 2, output.len());
    assert_eq!(in_near.len(), input.len());
    assert_eq!(in_far.len(), input.len());

    let middle = output.len() / 2;

    let (out_top, out_bottom) = output.split_at_mut(middle);

    for __ms_work_0 in
        crate::storage::polled(input.iter().zip(in_near.iter()).zip(out_top), control, 2)
    {
        let ((near, far), x) = __ms_work_0?;
        *x = (((3 * near) + 2) + far) >> 2;
    }

    for __ms_work_1 in
        crate::storage::polled(input.iter().zip(in_far.iter()).zip(out_bottom), control, 2)
    {
        let ((near, far), x) = __ms_work_1?;
        *x = (((3 * near) + 2) + far) >> 2;
    }

    Ok(())
}

pub fn upsample_hv_controlled(
    input: &[i16],
    in_near: &[i16],
    in_far: &[i16],
    scratch_space: &mut [i16],
    output: &mut [i16],
    control: &Control,
) -> Result<(), DecodeErrors> {
    storage::step(control, 0)?;

    assert_eq!(input.len() * 4, output.len());

    assert!(input.len() * 2 <= scratch_space.len());
    let scratch_space = &mut scratch_space[..input.len() * 2];

    let mut t = [0];
    upsample_vertical_controlled(input, in_near, in_far, &mut t, scratch_space, control)?;
    // horizontal upsampling must be done separate for every line
    // Otherwise it introduces artifacts that may cause the edge colors
    // to appear on the other line.

    // Since this is called for two scanlines/widths currently
    // splitting the inputs and outputs into half ensures we only handle
    // one scanline per iteration
    let scratch_half = scratch_space.len() / 2;

    let output_half = output.len() / 2;

    upsample_horizontal_controlled(
        &scratch_space[..scratch_half],
        &[],
        &[],
        &mut t,
        &mut output[..output_half],
        control,
    )?;

    upsample_horizontal_controlled(
        &scratch_space[scratch_half..],
        &[],
        &[],
        &mut t,
        &mut output[output_half..],
        control,
    )?;

    Ok(())
}

pub fn upsample_generic_controlled(
    input: &[i16],
    _in_near: &[i16],
    _in_far: &[i16],
    _scratch_space: &mut [i16],
    output: &mut [i16],
    control: &Control,
) -> Result<(), DecodeErrors> {
    storage::step(control, 0)?;
    // use nearest sample
    let difference = output.len() / input.len();
    if difference > 0 {
        for __ms_work_0 in crate::storage::polled(
            input.iter().zip(output.chunks_exact_mut(difference)),
            control,
            4,
        ) {
            let (input, chunk_output) = __ms_work_0?;
            if chunk_output.len() <= 4 {
                chunk_output.iter_mut().for_each(|x| *x = *input);
            } else {
                storage::fill(control, chunk_output, *input)?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
pub fn upsample_horizontal(
    input: &[i16],
    _ref: &[i16],
    _in_near: &[i16],
    _scratch: &mut [i16],
    output: &mut [i16],
) {
    upsample_horizontal_controlled(input, _ref, _in_near, _scratch, output, &Control::plain())
        .expect("plain upsampling has no resource failure");
}

#[cfg(test)]
pub fn upsample_vertical(
    input: &[i16],
    in_near: &[i16],
    in_far: &[i16],
    _scratch_space: &mut [i16],
    output: &mut [i16],
) {
    upsample_vertical_controlled(
        input,
        in_near,
        in_far,
        _scratch_space,
        output,
        &Control::plain(),
    )
    .expect("plain upsampling has no resource failure");
}

#[cfg(test)]
pub fn upsample_hv(
    input: &[i16],
    in_near: &[i16],
    in_far: &[i16],
    scratch_space: &mut [i16],
    output: &mut [i16],
) {
    upsample_hv_controlled(
        input,
        in_near,
        in_far,
        scratch_space,
        output,
        &Control::plain(),
    )
    .expect("plain upsampling has no resource failure");
}

#[cfg(test)]
pub fn upsample_generic(
    input: &[i16],
    _in_near: &[i16],
    _in_far: &[i16],
    _scratch_space: &mut [i16],
    output: &mut [i16],
) {
    upsample_generic_controlled(
        input,
        _in_near,
        _in_far,
        _scratch_space,
        output,
        &Control::plain(),
    )
    .expect("plain upsampling has no resource failure");
}
