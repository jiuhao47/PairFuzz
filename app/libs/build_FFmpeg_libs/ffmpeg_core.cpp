// Core implementation for FFmpeg H.264 parsing.
// This file contains the heavy parse implementation (helpers and
// h264_parse_init).

#include <stddef.h>
#include <stdint.h>
extern "C" {
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/imgutils.h>
#}

#include <assert.h>
#include <ctype.h>
#include <dirent.h>
#include <fstream>
#include <iostream>
#include <limits.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <string>
#include <unistd.h>
#include <vector>

#define IMG_WIDTH 1920
#define IMG_HEIGHT 1200
#define INBUF_SIZE IMG_WIDTH *IMG_HEIGHT

int decode_internal(AVCodecContext *av_codec_ctx, AVFrame *av_frm,
                    AVPacket *av_pkt, FILE *fp_out) {
  static char buf[1024];
  int ret;
  ret = avcodec_send_packet(av_codec_ctx, av_pkt);
  if (ret < 0) {
    fprintf(stderr, "Error sending a packet for decoding, error code:%d\n",
            ret);
    return -1;
  }

  while (ret >= 0) {
    ret = avcodec_receive_frame(av_codec_ctx, av_frm);
    if (ret == AVERROR(EAGAIN) || ret == AVERROR_EOF) {
      return 0;
    } else if (ret < 0) {
      fprintf(stderr, "Error during decoding, error code:%d\n", ret);
      return -1;
    }

    fflush(stdout);
    return 0;
  }
}

// Exported wrapper that captures stderr and calls the core parser
// h264_parse_init.
extern "C" int ffmpeg_h264_parse(const uint8_t *data, size_t size) {
  FILE *tempFile = tmpfile();
  if (tempFile == NULL) {
    perror("tmpfile");
    return 1;
  }

  int oldStderrFd = dup(fileno(stderr));
  if (oldStderrFd == -1) {
    perror("dup");
    fclose(tempFile);
    return 1;
  }

  if (dup2(fileno(tempFile), fileno(stderr)) == -1) {
    perror("dup2");
    close(oldStderrFd);
    fclose(tempFile);
    return 1;
  }

  int ret = (h264_parse_init(data, size) == 0) ? 0 : 1;

  if (dup2(oldStderrFd, fileno(stderr)) == -1) {
    perror("dup2");
    close(oldStderrFd);
    fclose(tempFile);
    return 1;
  }
  close(oldStderrFd);
  return ret;
}

extern "C" int h264_parse_init(const uint8_t *input_data, size_t input_size) {
  const AVCodec *av_codec;
  AVCodecParserContext *av_parser;
  AVCodecContext *av_codec_ctx = NULL;
  AVFrame *av_frame;
  int ret;
  int eof = 0;
  AVPacket *av_pkt;

  uint8_t *inbuf = (uint8_t *)malloc(
      (INBUF_SIZE + AV_INPUT_BUFFER_PADDING_SIZE) * sizeof(uint8_t));
  if (!inbuf) {
    fprintf(stderr, "Error! alloc inbuf failed");
    return 1;
  }
  memset(inbuf + INBUF_SIZE, 0, AV_INPUT_BUFFER_PADDING_SIZE);
  av_codec = avcodec_find_decoder(AV_CODEC_ID_H264);
  if (!av_codec) {
    fprintf(stderr, "Codec not found\n");
    free(inbuf);
    return 2;
  }

  av_codec_ctx = avcodec_alloc_context3(av_codec);
  if (!av_codec_ctx) {
    fprintf(stderr, "Could not allocate video codec context\n");
    free(inbuf);
    return 3;
  }

  av_pkt = av_packet_alloc();
  if (!av_pkt) {
    fprintf(stderr, "Error! alloc pkt failed");
    avcodec_free_context(&av_codec_ctx);
    free(inbuf);
    return 4;
  }

  av_parser = av_parser_init(av_codec->id);
  if (!av_parser) {
    fprintf(stderr, "parser not found\n");
    avcodec_free_context(&av_codec_ctx);
    av_frame_free(&av_frame);
    free(inbuf);
    return 5;
  }

  av_frame = av_frame_alloc();
  if (!av_frame) {
    fprintf(stderr, "Could not allocate video frame\n");
    av_parser_close(av_parser);
    avcodec_free_context(&av_codec_ctx);
    av_frame_free(&av_frame);
    av_packet_free(&av_pkt);
    free(inbuf);
    return 6;
  }

  if (avcodec_open2(av_codec_ctx, av_codec, NULL) < 0) {
    fprintf(stderr, "Could not open codec\n");
    av_parser_close(av_parser);
    avcodec_free_context(&av_codec_ctx);
    av_frame_free(&av_frame);
    av_packet_free(&av_pkt);
    free(inbuf);
    return 7;
  }

  const uint8_t *data = input_data;
  size_t data_size = input_size;
  size_t buffer_offset = 0;

  do {
    size_t to_copy = (data_size - buffer_offset < INBUF_SIZE)
                         ? (data_size - buffer_offset)
                         : INBUF_SIZE;
    memcpy(inbuf, data + buffer_offset, to_copy);
    buffer_offset += to_copy;
    data = inbuf;
    size_t current_size = to_copy;
    eof = !current_size;
    while (current_size > 0 || eof) {
      ret = av_parser_parse2(av_parser, av_codec_ctx, &av_pkt->data,
                             &av_pkt->size, data, current_size, AV_NOPTS_VALUE,
                             AV_NOPTS_VALUE, 0);

      if (ret < 0) {
        fprintf(stderr, "Error! cannot parse input data\n");
        av_parser_close(av_parser);
        avcodec_free_context(&av_codec_ctx);
        av_frame_free(&av_frame);
        av_packet_free(&av_pkt);
        free(inbuf);
        return 9;
      }

      data += ret;
      current_size -= ret;

      if (av_pkt->size) {
        if (decode_internal(av_codec_ctx, av_frame, av_pkt, NULL) < 0) {
          av_parser_close(av_parser);
          avcodec_free_context(&av_codec_ctx);
          av_frame_free(&av_frame);
          av_packet_free(&av_pkt);
          free(inbuf);
          return -1;
        }

      } else if (eof) {
        break;
      }
    }

  } while (!eof);

  if (decode_internal(av_codec_ctx, av_frame, NULL, NULL) < 0) {
    printf("decode_internal failed.\n");
    av_parser_close(av_parser);
    avcodec_free_context(&av_codec_ctx);
    av_frame_free(&av_frame);
    av_packet_free(&av_pkt);
    free(inbuf);
    return 10;
  }

  av_parser_close(av_parser);
  avcodec_free_context(&av_codec_ctx);
  av_frame_free(&av_frame);
  av_packet_free(&av_pkt);
  free(inbuf);
  return 0;
}
