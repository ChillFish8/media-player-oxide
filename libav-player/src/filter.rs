use std::ffi::CString;
use std::ptr;

use rusty_ffmpeg::ffi as ffmpeg;

use crate::codec::VideoDecoder;
use crate::{OutputPixelFormat, error};

/// Creates the video decoder filter pipeline.
///
/// This is responsible for converting the hardware frames to the target pixel format
/// which may be done either in hardware or software depending on support.
pub(crate) fn create_video_filter_pipeline(
    video_decoder: &VideoDecoder,
) -> Result<VideoFilterPipeline, error::FFmpegError> {
    tracing::debug!(
        target_pixel_formats = ?video_decoder.output_pixel_formats(),
        "creating filter pipeline",
    );

    let mut pipeline = VideoFilterPipeline::new()?;
    let mut inputs = ptr::null_mut();
    let mut outputs = ptr::null_mut();

    let buffer_src_args = video_decoder.filter_input_args();
    let buffer_src = unsafe { ffmpeg::avfilter_get_by_name(c"buffer".as_ptr()) };
    let buffer_sink = unsafe { ffmpeg::avfilter_get_by_name(c"buffersink".as_ptr()) };

    let filter = video_decoder.build_filter_args();
    tracing::debug!(filter = ?filter, "got filter graph");
    let filter_graph_str = CString::new(filter).unwrap();

    unsafe {
        let result = ffmpeg::avfilter_graph_create_filter(
            &raw mut pipeline.buffer_src_ctx,
            buffer_src,
            c"in".as_ptr(),
            buffer_src_args.as_ptr(),
            ptr::null_mut(),
            pipeline.filter_graph,
        );
        error::convert_ff_result(result)?;
        tracing::debug!("filter input created");

        let params = ffmpeg::av_buffersrc_parameters_alloc();
        (*params).hw_frames_ctx = video_decoder.hw_frames_ctx();
        ffmpeg::av_buffersrc_parameters_set(pipeline.buffer_src_ctx, params);
        ffmpeg::av_free(params.cast());

        let result = ffmpeg::avfilter_graph_create_filter(
            &raw mut pipeline.buffer_sink_ctx,
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
                pipeline.buffer_sink_ctx,
                0,
            );
            o = (*o).next;
        }
        tracing::debug!("linked inputs");

        let mut i = inputs;
        while !i.is_null() {
            let inp = &*i;
            ffmpeg::avfilter_link(
                pipeline.buffer_src_ctx,
                0,
                inp.filter_ctx,
                inp.pad_idx as u32,
            );
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
                (*ctx).hw_device_ctx = video_decoder.hw_frames_ctx();
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
/// the target pixel format ([OutputPixelFormat](OutputPixelFormat))
///
/// The way this is done varies depending on the accelerator but currently conversion
/// is done via the default `format` filter.
pub struct VideoFilterPipeline {
    filter_graph: *mut ffmpeg::AVFilterGraph,
    buffer_src_ctx: *mut ffmpeg::AVFilterContext,
    buffer_sink_ctx: *mut ffmpeg::AVFilterContext,
}

impl VideoFilterPipeline {
    fn new() -> Result<Self, error::FFmpegError> {
        let filter_graph = unsafe { ffmpeg::avfilter_graph_alloc() };
        if filter_graph.is_null() {
            Err(error::FFmpegError::custom(
                "failed to allocate filter graph",
            ))
        } else {
            Ok(Self {
                filter_graph,
                buffer_src_ctx: ptr::null_mut(),
                buffer_sink_ctx: ptr::null_mut(),
            })
        }
    }

    pub(crate) fn write_frame(
        &mut self,
        frame: &mut ffmpeg::AVFrame,
    ) -> Result<(), error::FFmpegError> {
        let result = unsafe {
            ffmpeg::av_buffersrc_add_frame_flags(
                self.buffer_src_ctx,
                frame,
                ffmpeg::AV_BUFFERSRC_FLAG_KEEP_REF as i32,
            )
        };
        error::convert_ff_result(result)?;
        Ok(())
    }

    pub(crate) fn read_frame(
        &mut self,
        frame: &mut ffmpeg::AVFrame,
    ) -> Result<(), error::FFmpegError> {
        let result =
            unsafe { ffmpeg::av_buffersink_get_frame(self.buffer_sink_ctx, frame) };
        error::convert_ff_result(result)?;
        Ok(())
    }
}

impl Drop for VideoFilterPipeline {
    fn drop(&mut self) {
        if !self.filter_graph.is_null() {
            unsafe { ffmpeg::avfilter_graph_free(&raw mut self.filter_graph) };
        }
    }
}
