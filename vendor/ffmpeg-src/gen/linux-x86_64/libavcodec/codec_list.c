static const FFCodec * const codec_list[] = {
    &ff_av1_nvenc_encoder,
    &ff_av1_vaapi_encoder,
    &ff_h264_nvenc_encoder,
    &ff_h264_vaapi_encoder,
    &ff_hevc_nvenc_encoder,
    &ff_hevc_vaapi_encoder,
    &ff_h264_decoder,
    &ff_hevc_decoder,
    &ff_vp9_decoder,
    &ff_av1_decoder,
    NULL };
