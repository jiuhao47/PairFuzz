#include <stdint.h>
#include <stddef.h>
extern "C"{
    #include <libavcodec/avcodec.h>
    #include <libavformat/avformat.h>
    #include <libavutil/imgutils.h>
}

#include <string>
#include <vector>
#include <stdio.h>
#include <assert.h>
#include <stdint.h>
#include <iostream>
#include <fstream>
#include <string>
#include <vector>
#include <stdlib.h>
#include <string.h>
#include <dirent.h>
#include <limits.h>
#include <stdarg.h>
#include <stddef.h>
#include <ctype.h>

int decode_internal(AVCodecContext* av_codec_ctx, AVFrame* av_frm, AVPacket* av_pkt, FILE* fp_out) {
    static char buf[1024];
    int ret;
    // 将当前帧送入到解码器当中去解码
    ret = avcodec_send_packet(av_codec_ctx, av_pkt);
    if (ret < 0) {
        fprintf(stderr, "Error sending a packet for decoding, error code:%d\n", ret);
        return -1;
    }

    while (ret >= 0) {
        // 获取已经解码的数据
        ret = avcodec_receive_frame(av_codec_ctx, av_frm);
        if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
            return 0;
        } else if (ret < 0) {
            fprintf(stderr, "Error during decoding, error code:%d\n", ret);
            return -1;
        }

        fprintf(stderr, "saving frame %3" PRId64 "\n", av_codec_ctx->frame_num);
        fflush(stdout);
        return 0;
        // 将已经解码的数据存储到文件中
    //     int size = av_frm->width * av_frm->height;
    //     if (av_frm->data[0] && av_frm->data[1] && av_frm->data[2]) {
    //         fwrite(av_frm->data[0], 1, size, fp_out); // Y
    //         fwrite(av_frm->data[1], 1, size / 4, fp_out); // U
    //         fwrite(av_frm->data[2], 1, size / 4, fp_out); // V
    //     }
 }
}

extern "C" int h264_parse(const uint8_t *data, size_t size) {
    const AVCodec *codec = NULL;
    AVCodecContext *codec_ctx = NULL;
    AVFrame* av_frame;
    AVCodecParserContext* av_parser;
    AVPacket *packet;
    int ret;
    int success = 10;
    av_log_set_level(AV_LOG_QUIET);

    // Initialize libavcodec
    // avcodec_register_all();

    // Find the H.264 video decoder
    codec = avcodec_find_decoder(AV_CODEC_ID_H264);
    if (!codec) {
        fprintf(stderr, "Codec not found\n");
        return 1;
    }

    // create ctx
    codec_ctx = avcodec_alloc_context3(codec);
    if (!codec_ctx) {
        fprintf(stderr, "Could not allocate video codec context\n");
        return 2;
    }

    // create pkt
    packet = av_packet_alloc();
    if (!packet) {
        fprintf(stderr, "Error! alloc pkt failed\n");
        avcodec_free_context(&codec_ctx);
        
        return 3;
    }

    // parse codec info
    av_parser = av_parser_init(codec->id);
    if (!av_parser) {
        fprintf(stderr, "parser not found\n");
        av_parser_close(av_parser);
        avcodec_free_context(&codec_ctx);
        av_frame_free(&av_frame);
        
        return 4;
    }

    // create frame
    av_frame = av_frame_alloc();
    if (!av_frame) {
        fprintf(stderr, "Could not allocate video frame\n");
        av_parser_close(av_parser);
        avcodec_free_context(&codec_ctx);
        av_frame_free(&av_frame);
        av_packet_free(&packet);
        return 5;
    }

    // open dec codec
    if (avcodec_open2(codec_ctx, codec, NULL) < 0) {
        fprintf(stderr, "Could not open codec\n");
        av_parser_close(av_parser);
        avcodec_free_context(&codec_ctx);
        av_frame_free(&av_frame);
        av_packet_free(&packet);
        return 6;
    }
    // uint8_t* data_ac = &data;
    while (size > 0) {
        printf("packet->size is %d, size is %d\n",packet->size,size);
        ret = av_parser_parse2(av_parser, codec_ctx, &packet->data, &packet->size,
                               data, size, AV_NOPTS_VALUE, AV_NOPTS_VALUE, 0);

        if (ret < 0) {
            fprintf(stderr, "Error! can not parse input data\n");
            av_parser_close(av_parser);
                avcodec_free_context(&codec_ctx);
                av_frame_free(&av_frame);
                av_packet_free(&packet);
            return 7;
        }

        // data += ret;
        size -= ret;

        if (packet->size) {
            // decode
            if(decode_internal(codec_ctx, av_frame, packet, stdout)<0)
            {   
                av_parser_close(av_parser);
                avcodec_free_context(&codec_ctx);
                av_frame_free(&av_frame);
                av_packet_free(&packet);

                return -1;
            }
    }}

    // flush the decoder
    if(decode_internal(codec_ctx, av_frame, NULL, stdout)<0){
        av_parser_close(av_parser);
        avcodec_free_context(&codec_ctx);
        av_frame_free(&av_frame);
        av_packet_free(&packet);
        return -1;
    }

    av_parser_close(av_parser);
    avcodec_free_context(&codec_ctx);
    av_frame_free(&av_frame);
    av_packet_free(&packet);

    return 0;
}

int main(int argc, char **argv) {
    std::ifstream in(argv[1]);
    in.seekg(0, in.end);
    size_t length = in.tellg();
    in.seekg(0, in.beg);
    std::vector<char> bytes(length);
    in.read(bytes.data(), bytes.size());

    int ret = h264_parse(reinterpret_cast<const uint8_t *>(bytes.data()), bytes.size());

    printf("ret: %d\n", ret);

    return 0;
}
