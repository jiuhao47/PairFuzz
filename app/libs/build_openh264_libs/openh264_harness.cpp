/*
# Copyright 2021 Google LLC
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#      http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
#
################################################################################
*/
// TODO: This should be moved to the openh264 repo.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <memory>

#include "codec_def.h"
#include "codec_app_def.h"
#include "codec_api.h"
#include "read_config.h"
#include "typedefs.h"
#include "measure_time.h"


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
/*
 * To build locally:
 * CC=clang CXX=clang++ CFLAGS="-fsanitize=address,fuzzer-no-link -g" CXXFLAGS="-fsanitize=address,fuzzer-no-link -g" LDFLAGS="-fsanitize=address,fuzzer-no-link" make -j$(nproc) USE_ASM=No BUILDTYPE=Debug libraries
 * clang++ -o decoder_fuzzer -fsanitize=address -g -O1 -I./codec/api/wels -I./codec/console/common/inc -I./codec/common/inc -L. -lFuzzer -lstdc++ decoder_fuzzer.cpp libopenh264.a
 */

// ...existing code...

extern "C" int openh264_parse(const uint8_t *data, size_t size)
{
  int32_t i;
  int32_t iBufPos = 0;
  int32_t iEndOfStreamFlag;
  int iLevelSetting = (int) WELS_LOG_QUIET; // disable logging while fuzzing
  int32_t iSliceSize;
  ISVCDecoder *pDecoder;
  SDecodingParam sDecParam = {0};
  SBufferInfo sDstBufInfo;
  std::unique_ptr<uint8_t[]> pBuf(new uint8_t[size + 4]);
  uint8_t* pData[3] = {NULL};
  uint8_t uiStartCode[4] = {0, 0, 0, 1};

  memcpy(pBuf.get(), data, size);
  memcpy(pBuf.get() + size, &uiStartCode[0], 4);
  memset(&sDstBufInfo, 0, sizeof(SBufferInfo));

  // Set decoding parameters
  sDecParam.eEcActiveIdc = ERROR_CON_SLICE_COPY;
  sDecParam.sVideoProperty.eVideoBsType = VIDEO_BITSTREAM_AVC;
  
  // Create and initialize the decoder
  WelsCreateDecoder (&pDecoder);
  pDecoder->Initialize (&sDecParam);
  pDecoder->SetOption (DECODER_OPTION_TRACE_LEVEL, &iLevelSetting);

  bool success = 1; // Assume success initially

  while (1) {
    if (iBufPos >= size) {
      iEndOfStreamFlag = 1;
      if (iEndOfStreamFlag)
        pDecoder->SetOption (DECODER_OPTION_END_OF_STREAM, (void*)&iEndOfStreamFlag);
      break;
    }

    // Find the next NAL unit
    for (i = 0; i < size; i++) {
      if ((pBuf[iBufPos + i] == 0 && pBuf[iBufPos + i + 1] == 0 && pBuf[iBufPos + i + 2] == 0 && pBuf[iBufPos + i + 3] == 1
          && i > 0) || (pBuf[iBufPos + i] == 0 && pBuf[iBufPos + i + 1] == 0 && pBuf[iBufPos + i + 2] == 1 && i > 0)) {
        break;
      }
    }
    iSliceSize = i;

    if (iSliceSize < 4) {
      if (iSliceSize == 0) {
        success = 0; // Invalid slice size, mark as failure
        goto label_cleanup;
      }
      iBufPos += iSliceSize;
      continue;
    }

    // Decode the frame
    int ret = pDecoder->DecodeFrameNoDelay(pBuf.get() + iBufPos, iSliceSize, pData, &sDstBufInfo);
    // printf("[X]:ret is %d, sDstBufInfo.iBufferStatus is %d\n", ret, sDstBufInfo.iBufferStatus);
    // Check if decoding failed
    if (ret != 0 && ret !=16 ) {
    //   printf("!!!ret is %d, sDstBufInfo.iBufferStatus is %d\n", ret, sDstBufInfo.iBufferStatus);
      success = 0; // Mark as failure if decoding failed
      goto label_cleanup;
    }

    iBufPos += iSliceSize;
  }

label_cleanup:
  pDecoder->Uninitialize ();
  WelsDestroyDecoder (pDecoder);

  // Print "success" only if the entire input was successfully decoded
//   if (success) {
//     printf("success\n");
//   }

  return (success==1) ? 0 : 1;
}
extern "C" int diffRayTestOneInput_2(const uint8_t *data, size_t size) {
  return openh264_parse(data, size);
}