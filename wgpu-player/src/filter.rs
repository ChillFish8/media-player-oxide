use std::ffi::CString;
use std::ptr;

use rusty_ffmpeg::ffi as ffmpeg;

use crate::codec::VideoDecoder;
use crate::{OutputPixelFormat, error};

/// Creates the video decoder filter pipeline.
///
/// This is responsible for converting the hardware frames to the target pixel format
/// which may be done either in hardware or software depending on support.
pub(crate) fn create_filter_pipeline(
    video_decoder: &VideoDecoder,
    target_pixel_format: OutputPixelFormat,
) -> Result<VideoFilterPipeline, error::FFmpegError> {
    tracing::debug!(target_pixel_format = ?target_pixel_format, "creating filter pipeline");

    let pipeline = VideoFilterPipeline::new()?;
    let mut inputs = ptr::null_mut();
    let mut outputs = ptr::null_mut();

    let mut buffer_src_ctx: *mut ffmpeg::AVFilterContext = ptr::null_mut();
    let mut buffer_sink_ctx: *mut ffmpeg::AVFilterContext = ptr::null_mut();

    let buffer_src_args = video_decoder.filter_input_args();
    let buffer_src = unsafe { ffmpeg::avfilter_get_by_name(c"buffer".as_ptr()) };
    let buffer_sink = unsafe { ffmpeg::avfilter_get_by_name(c"buffersink".as_ptr()) };

    let filter = video_decoder
        .accelerator()
        .map(|accel| accel.build_filter_graph(target_pixel_format))
        .unwrap_or_else(|| format!("format={}", target_pixel_format.to_filter_name()));
    tracing::debug!(filter = ?filter, "got filter graph");
    let filter_graph_str = CString::new(filter).unwrap();

    unsafe {
        let result = ffmpeg::avfilter_graph_create_filter(
            &raw mut buffer_src_ctx,
            buffer_src,
            c"in".as_ptr(),
            buffer_src_args.as_ptr(),
            ptr::null_mut(),
            pipeline.filter_graph,
        );
        error::convert_ff_result(result)?;
        tracing::debug!("filter input created");

        let params = ffmpeg::av_buffersrc_parameters_alloc();
        (*params).hw_frames_ctx = dbg!(video_decoder.hw_frame_ctx());
        ffmpeg::av_buffersrc_parameters_set(buffer_src_ctx, params);
        ffmpeg::av_free(params.cast());

        let result = ffmpeg::avfilter_graph_create_filter(
            &raw mut buffer_sink_ctx,
            buffer_sink,
            c"out".as_ptr(),
            ptr::null(),
            ptr::null_mut(),
            pipeline.filter_graph,
        );
        error::convert_ff_result(result)?;
        tracing::debug!("filter output created");

        let result = ffmpeg::avfilter_graph_parse2(
            pipeline.filter_graph,
            filter_graph_str.as_ptr(),
            &raw mut inputs,
            &raw mut outputs,
        );
        let result = error::convert_ff_result(result);
        if result.is_err() {
            ffmpeg::avfilter_inout_free(&raw mut inputs);
            ffmpeg::avfilter_inout_free(&raw mut outputs);
            result?;
        }
        tracing::debug!("parsed filter graph");

        let mut o = outputs;
        while !o.is_null() {
            let filter_out = &*o;
            ffmpeg::avfilter_link(
                filter_out.filter_ctx,
                filter_out.pad_idx as u32,
                buffer_sink_ctx,
                0,
            );
            o = (*o).next;
        }
        tracing::debug!("linked inputs");

        let mut i = inputs;
        while !i.is_null() {
            let inp = &*i;
            ffmpeg::avfilter_link(buffer_src_ctx, 0, inp.filter_ctx, inp.pad_idx as u32);
            i = inp.next;
        }
        tracing::debug!("linked outputs");

        ffmpeg::avfilter_inout_free(&raw mut inputs);
        ffmpeg::avfilter_inout_free(&raw mut outputs);

        // Attach hardware context if available to the filters.
        let graph = &mut *pipeline.filter_graph;
        for i in 0..graph.nb_filters {
            let ctx = *graph.filters.offset(i as isize);
            assert!(!ctx.is_null());

            tracing::debug!("filter_stage: {:?}", std::ffi::CStr::from_ptr((*ctx).name));
            if (*(*ctx).filter).flags as u32 & ffmpeg::AVFILTER_FLAG_HWDEVICE != 0 {
                (*ctx).hw_device_ctx = video_decoder.hw_device_ctx();
            }
        }
        tracing::debug!("attached hardware context");

        let result =
            ffmpeg::avfilter_graph_config(pipeline.filter_graph, ptr::null_mut());
        error::convert_ff_result(result)?;
    };

    tracing::debug!("created filter pipeline");

    Ok(pipeline)
}

/// A chain of filters responsible for converting from the hardware frames to
/// the target pixel format ([OutputPixelFormat](crate::OutputPixelFormat))
///
/// The way this is done varies depending on the accelerator but currently conversion
/// is done via the default `format` filter.
pub struct VideoFilterPipeline {
    filter_graph: *mut ffmpeg::AVFilterGraph,
}

impl VideoFilterPipeline {
    fn new() -> Result<Self, error::FFmpegError> {
        let filter_graph = unsafe { ffmpeg::avfilter_graph_alloc() };
        if filter_graph.is_null() {
            Err(error::FFmpegError::custom(
                "failed to allocate filter graph",
            ))
        } else {
            Ok(Self { filter_graph })
        }
    }
}

impl Drop for VideoFilterPipeline {
    fn drop(&mut self) {
        if !self.filter_graph.is_null() {
            unsafe { ffmpeg::avfilter_graph_free(&raw mut self.filter_graph) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AcceleratorConfig, InputSource, MediaType};

    #[test]
    fn test_video_filter_construction() {
        let _ = tracing_subscriber::fmt::try_init();

        let source = InputSource::open_file("../media/test.mp4").unwrap();

        let stream = source
            .find_best_stream(MediaType::Video, None)
            .unwrap()
            .unwrap();

        let accelerator_config = AcceleratorConfig::default();
        let mut decoder = stream.open_decoder(&accelerator_config).unwrap();

        let pipeline = create_filter_pipeline(&decoder, OutputPixelFormat::Nv12)
            .expect("filter should be created successfully");
    }
}
