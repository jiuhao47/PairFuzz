// Core implementation for openh264 parsing.
// This file contains the parsing logic moved out from the original harness.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <memory>

#include "codec_api.h"
#include "codec_app_def.h"
#include "codec_def.h"
#include "measure_time.h"
#include "read_config.h"
#include "typedefs.h"

extern "C" int openh264_parse(const uint8_t *data, size_t size) {
  int32_t i;
  int32_t iBufPos = 0;
  int32_t iEndOfStreamFlag;
  int iLevelSetting = (int)WELS_LOG_QUIET; // disable logging while fuzzing
  int32_t iSliceSize;
  ISVCDecoder *pDecoder;
  SDecodingParam sDecParam = {0};
  SBufferInfo sDstBufInfo;
  std::unique_ptr<uint8_t[]> pBuf(new uint8_t[size + 4]);
  uint8_t *pData[3] = {NULL};
  uint8_t uiStartCode[4] = {0, 0, 0, 1};

  memcpy(pBuf.get(), data, size);
  memcpy(pBuf.get() + size, &uiStartCode[0], 4);
  memset(&sDstBufInfo, 0, sizeof(SBufferInfo));

  sDecParam.eEcActiveIdc = ERROR_CON_SLICE_COPY;
  sDecParam.sVideoProperty.eVideoBsType = VIDEO_BITSTREAM_AVC;
  WelsCreateDecoder(&pDecoder);
  pDecoder->Initialize(&sDecParam);
  pDecoder->SetOption(DECODER_OPTION_TRACE_LEVEL, &iLevelSetting);

  bool success = 1;

  while (1) {
    if (iBufPos >= size) {
      iEndOfStreamFlag = 1;
      if (iEndOfStreamFlag)
        pDecoder->SetOption(DECODER_OPTION_END_OF_STREAM,
                            (void *)&iEndOfStreamFlag);
      break;
    }

    for (i = 0; i < size; i++) {
      if ((pBuf[iBufPos + i] == 0 && pBuf[iBufPos + i + 1] == 0 &&
           pBuf[iBufPos + i + 2] == 0 && pBuf[iBufPos + i + 3] == 1 && i > 0) ||
          (pBuf[iBufPos + i] == 0 && pBuf[iBufPos + i + 1] == 0 &&
           pBuf[iBufPos + i + 2] == 1 && i > 0)) {
        break;
      }
    }
    iSliceSize = i;

    if (iSliceSize < 4) {
      if (iSliceSize == 0) {
        success = 0;
        goto label_cleanup;
      }
      iBufPos += iSliceSize;
      continue;
    }

    int ret = pDecoder->DecodeFrameNoDelay(pBuf.get() + iBufPos, iSliceSize,
                                           pData, &sDstBufInfo);
    if (ret != 0 && ret != 16) {
      success = 0;
      goto label_cleanup;
    }

    iBufPos += iSliceSize;
  }

label_cleanup:
  pDecoder->Uninitialize();
  WelsDestroyDecoder(pDecoder);

  return (success == 1) ? 0 : 1;
}
