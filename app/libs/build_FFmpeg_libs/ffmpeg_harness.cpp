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
#include <unistd.h> // 添加此行以包含 dup、dup2 和 close 函数
#define IMG_WIDTH 1920
#define IMG_HEIGHT 1200
#define INBUF_SIZE IMG_WIDTH * IMG_HEIGHT

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

        // fprintf(stderr, "saving frame %3" PRId64 "\n", av_codec_ctx->frame_num);
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

extern "C" int h264_parse_init(const uint8_t* input_data, size_t input_size)
{
	const AVCodec* av_codec;
	AVCodecParserContext* av_parser;
	AVCodecContext* av_codec_ctx = NULL;
	AVFrame* av_frame;
	int ret;
	int eof = 0;
	AVPacket* av_pkt;
	

    // malloc input buffer
	uint8_t* inbuf = (uint8_t*)malloc((INBUF_SIZE + AV_INPUT_BUFFER_PADDING_SIZE) * sizeof(uint8_t));
	if (!inbuf) {
		fprintf(stderr, "Error! alloc inbuf failed");
		return 1;
	}
	memset(inbuf + INBUF_SIZE, 0, AV_INPUT_BUFFER_PADDING_SIZE);
	// 创建 h264 解码器
	av_codec = avcodec_find_decoder(AV_CODEC_ID_H264);
	if (!av_codec) {
		fprintf(stderr, "Codec not found\n");
        free(inbuf);
		return 2;
	}

	// 创建解码上下文
	av_codec_ctx = avcodec_alloc_context3(av_codec);
	if (!av_codec_ctx) {
		fprintf(stderr, "Could not allocate video codec context\n");
		free(inbuf);
        return 3;
	}

	// 创建 AVPacket
	av_pkt = av_packet_alloc();
	if (!av_pkt) {
		fprintf(stderr, "Error! alloc pkt failed");
		avcodec_free_context(&av_codec_ctx);
        free(inbuf);
        return 4;
	}

	// 创建解析器
	av_parser = av_parser_init(av_codec->id);
	if (!av_parser) {
		fprintf(stderr, "parser not found\n");
        avcodec_free_context(&av_codec_ctx);
        av_frame_free(&av_frame);
        free(inbuf);
        return 5;
	}

	// 创建解码帧
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
 
	// 打开解码器
	if (avcodec_open2(av_codec_ctx, av_codec, NULL) < 0) {
		fprintf(stderr, "Could not open codec\n");
		av_parser_close(av_parser);
        avcodec_free_context(&av_codec_ctx);
        av_frame_free(&av_frame);
        av_packet_free(&av_pkt);
        free(inbuf);
        return 7;
	}

	// 模拟读取输入数据
	const uint8_t* data = input_data;
	size_t data_size = input_size;
	size_t buffer_offset = 0;  // 缓冲区偏移量

	do {
		// 每次从数据流中填充缓冲区，模拟文件读取
		size_t to_copy = (data_size - buffer_offset < INBUF_SIZE) ? (data_size - buffer_offset) : INBUF_SIZE;
		memcpy(inbuf, data + buffer_offset, to_copy);
		buffer_offset += to_copy;
        // printf("to_copy is %zu\n",to_copy); 

        // printf("data size is %zu\n",data_size);
		/* 使用解析器来拆分数据 */
		data = inbuf;  // 使用缓冲区中的数据
		size_t current_size = to_copy;
        eof = !current_size;
		// 解析数据直到数据大小为0
		while (current_size > 0 || eof) {
            // printf("current size is %zu\n",current_size);
			ret = av_parser_parse2(av_parser, av_codec_ctx, &av_pkt->data, &av_pkt->size,
				data, current_size, AV_NOPTS_VALUE, AV_NOPTS_VALUE, 0);

			if (ret < 0) {
				fprintf(stderr, "Error! cannot parse input data\n");
				av_parser_close(av_parser);
                avcodec_free_context(&av_codec_ctx);
                av_frame_free(&av_frame);
                av_packet_free(&av_pkt);
                free(inbuf);
                return 9;
			}

			// 更新后续帧的起始地址
			data += ret;
			current_size -= ret;

			if (av_pkt->size) {
				// 解码
				if(decode_internal(av_codec_ctx, av_frame, av_pkt, NULL)<0){
                    av_parser_close(av_parser);
                    avcodec_free_context(&av_codec_ctx);
                    av_frame_free(&av_frame);
                    av_packet_free(&av_pkt);
                    free(inbuf);
                    return -1;
                }  //  printf("decode_internal failed.\n");
                
			}
			else if (eof) {
				break;
			}
		}

		// // 如果没有更多数据，标记为文件结束
		// if (data_size == 0) {
		// 	eof = 1;
		// }

	} while (!eof);

	// 解码剩余数据
	if(decode_internal(av_codec_ctx, av_frame, NULL, NULL)<0){
        printf("decode_internal failed.\n");
        av_parser_close(av_parser);
        avcodec_free_context(&av_codec_ctx);
        av_frame_free(&av_frame);
        av_packet_free(&av_pkt);
        free(inbuf);
        return 10;
    }  // 无需输出到文件

	// 清理资源
	av_parser_close(av_parser);
	avcodec_free_context(&av_codec_ctx);
	av_frame_free(&av_frame);
	av_packet_free(&av_pkt);
    free(inbuf);
	return 0;
}

extern "C" int ffmpeg_h264_parse(const uint8_t *data, size_t size) {
    // 创建一个临时文件来捕获标准错误输出
    FILE* tempFile = tmpfile();
    if (tempFile == NULL) {
        perror("tmpfile");
        return 1;
    }

    // 保存原始的标准错误文件描述符
    int oldStderrFd = dup(fileno(stderr));
    if (oldStderrFd == -1) {
        perror("dup");
        fclose(tempFile);
        return 1;
    }

    // 将标准错误输出重定向到临时文件
    if (dup2(fileno(tempFile), fileno(stderr)) == -1) {
        perror("dup2");
        close(oldStderrFd);
        fclose(tempFile);
        return 1;
    }

    int ret =(h264_parse_init(data, size)==0)?0:1;
// 恢复标准错误输出
if (dup2(oldStderrFd, fileno(stderr)) == -1) {
    perror("dup2");
    close(oldStderrFd);
    fclose(tempFile);
    return 1;
}
close(oldStderrFd);
    return ret;
}
extern "C" int diffRayTestOneInput_1(const uint8_t *data, size_t size) {
  return ffmpeg_h264_parse(data, size);
}
