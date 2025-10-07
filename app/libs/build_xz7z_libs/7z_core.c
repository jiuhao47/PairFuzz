#include "7zCrc.h"
#include "7zTypes.h"
#include "Alloc.h"
#include "Xz.h"
#include "XzCrc64.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct CLI_XZ {
  CXzUnpacker state;
  ECoderStatus status;
  unsigned char *next_in;
  unsigned char *next_out;
  SizeT avail_in;
  SizeT avail_out;
};

// extern int XZ_FILE_PARSER_START;
// int XZ_FILE_PARSER_START = 0;

// extern int ZIP_ERROR_CODE_DIFF_FUZZ;
// int ZIP_ERROR_CODE_DIFF_FUZZ = 0;
// extern int PATCH_FLAG;
// int PATCH_FLAG = 1;
// extern int FDL_DEBUG;
// int FDL_DEBUG = 1;

#define XZ_RESULT_OK 0
#define XZ_RESULT_DATA_ERROR 1
#define XZ_STREAM_END 2

#define XZ_DIC_HEURISTIC 3

#define CLI_XZ_OBUF_SIZE 1024 * 1024
// #define CLI_XZ_OBUF_SIZE 40
#define CLI_XZ_IBUF_SIZE CLI_XZ_OBUF_SIZE >> 2 /* compression ratio 25% */
#define CLI_MAX_ALLOCATION 1024 * 1024 * 1024

#define UNUSEDPARAM(x) (void)(x)

void *__xz_wrap_alloc(size_t size);
void __xz_wrap_free(void *freeme);

void *__xz_wrap_alloc(size_t size) {

  if (!size || size > CLI_MAX_ALLOCATION)
    return NULL;
  if (!size || size > CLI_MAX_ALLOCATION) {
    printf(
        "xz_iface: Attempt to allocate %lu bytes exceeds CLI_MAX_ALLOCATION.\n",
        (unsigned long int)size);
    return NULL;
  }
  return malloc(size);
}

void __xz_wrap_free(void *freeme) { free(freeme); }

static void *SzAlloc1(ISzAllocPtr p, size_t size) {
  UNUSEDPARAM(p);
  return __xz_wrap_alloc(size);
}
static void SzFree1(ISzAllocPtr p, void *address) {
  UNUSEDPARAM(p);
  __xz_wrap_free(address);
}
static ISzAlloc g_Alloc1 = {SzAlloc1, SzFree1};

int cli_XzInit(struct CLI_XZ *XZ) {
  XzUnpacker_Construct(&XZ->state, &g_Alloc1);

  if (g_Crc64Table[1] == 0) {
    // printf("[FDL] Crc generate table\n");
    Crc64GenerateTable();
    CrcGenerateTable();
  }

  return XZ_RESULT_OK;
}

void cli_XzShutdown(struct CLI_XZ *XZ) {
  if (!XZ)
    return;
  XzUnpacker_Free(&XZ->state);
}

int cli_XzDecode(struct CLI_XZ *XZ) {
  SRes res;
  SizeT outbytes, inbytes;
  ECoderStatus status;

  inbytes = XZ->avail_in;
  outbytes = XZ->avail_out;

  // printf("[FDL] before XzUnpacker_Code, outbytes:%ld\n", outbytes);
  // printf("[FDL] XZ->next_in[0]:%d, input size:%ld, inbytes:%ld\n",
  // XZ->next_in[0], XZ->avail_in, inbytes); res = XzUnpacker_Code(&XZ->state,
  // XZ->next_out, &outbytes,
  //                       XZ->next_in, &inbytes, 1, CODER_FINISH_ANY,
  //                       &XZ->status);
  res = XzUnpacker_Code(&XZ->state, XZ->next_out, &outbytes, XZ->next_in,
                        &inbytes, 0, CODER_FINISH_ANY, &XZ->status);

  // printf("[FDL] unpacker_code res:%d, outbytes:%ld, inbytes:%ld\n", res,
  // outbytes, inbytes); printf("[FDL] decode res: "); for (int
  // i=0;i<outbytes;i++)
  //     printf("%02x ", XZ->next_out[i]);
  // printf("\n");

  XZ->avail_in -= inbytes;
  XZ->next_in += inbytes;

  // keep the same, because we do not need to keep the output
  // XZ->avail_out = CLI_XZ_OBUF_SIZE;
  // XZ->next_out = XZ->next_out;

  if (XZ->status == CODER_STATUS_FINISHED_WITH_MARK ||
      XzUnpacker_IsStreamWasFinished(&XZ->state))
    return XZ_STREAM_END;

  if (XZ->status == CODER_STATUS_NOT_FINISHED && XZ->avail_out == 0)
    return XZ_RESULT_OK;

  if (((inbytes == 0) && (outbytes == 0)) || res != SZ_OK) {
    return XZ_RESULT_DATA_ERROR;
  }
  return XZ_RESULT_OK;
}

// #define CLI_XZ_OBUF_SIZE 1024*1024

extern int cli_scanxz(const unsigned char *data, unsigned int size) {
  int ret = 0;
  int rc;
  struct CLI_XZ strm; // from xz_iface.h
  size_t off = 0;
  size_t avail;
  unsigned char *buf;
  buf = malloc(CLI_XZ_OBUF_SIZE);
  if (buf == NULL) {
    // printf("cli_scanxz: nomemory for decompress buffer.\n");
    return 1;
  }
  memset(&strm, 0x00, sizeof(struct CLI_XZ));

  strm.next_out = buf;
  strm.avail_out = CLI_XZ_OBUF_SIZE;

  rc = cli_XzInit(&strm);

  if (rc != XZ_RESULT_OK) {
    // printf("cli_scanxz: DecompressInit failed: %i\n", rc);
    free(buf);
    return 2;
  }

  // printf("cli_scanxz: decompressing xz file\n");
  do {
    /* set up input buffer */
    if (!strm.avail_in) {
      // printf("set up input buffer\n");
      strm.next_in = (unsigned char *)data;
      strm.avail_in = size;
      if (!strm.avail_in) {
        // printf("cli_scanxz: premature end of compressed stream\n");
        ret = 3;
        cli_XzShutdown(&strm);
        free(buf);
        return ret;
      }
    }

    // printf("[FDL] before, available out :%ld\n", strm.avail_out);
    /* xz decompress a chunk */
    rc = cli_XzDecode(&strm);

    // printf("[FDL] after, available out :%ld, decode ret:%d\n",
    // strm.avail_out, rc);

    if (XZ_RESULT_OK != rc && XZ_STREAM_END != rc) {
      // printf("cli_scanxz: decompress error: %d\n", rc);
      ret = 4;
      cli_XzShutdown(&strm);
      free(buf);
      return ret;
    }
    /* write decompress buffer */
    if (!strm.avail_out || rc == XZ_STREAM_END) {
      // printf("reset output buf\n");
      strm.next_out = buf;
      strm.avail_out = CLI_XZ_OBUF_SIZE;
    }
  } while (XZ_STREAM_END != rc);

  cli_XzShutdown(&strm);
  free(buf);

  return ret;
}

unsigned char *readFileIntoBuffer(const char *filename, size_t *fileSize) {
  FILE *file = fopen(filename, "rb");
  if (!file) {
    fprintf(stderr, "Failed to open file: %s\n", filename);
    return NULL;
  }

  // Get the file size
  fseek(file, 0, SEEK_END);
  long size = ftell(file);
  fseek(file, 0, SEEK_SET);

  // Allocate memory for the buffer
  unsigned char *buffer = (unsigned char *)malloc(size);
  if (!buffer) {
    fclose(file);
    fprintf(stderr, "Failed to allocate memory for buffer.\n");
    return NULL;
  }

  // Read the file into the buffer
  size_t bytesRead = fread(buffer, sizeof(unsigned char), size, file);
  if (bytesRead != size) {
    fclose(file);
    free(buffer);
    fprintf(stderr, "Failed to read file: %s\n", filename);
    return NULL;
  }

  fclose(file);

  if (fileSize) {
    *fileSize = (size_t)size;
  }

  return buffer;
}
