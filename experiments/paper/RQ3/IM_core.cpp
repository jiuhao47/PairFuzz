// Core implementation for ImageMagick harness
#include <Magick++.h>
#include <Magick++/Blob.h>
#include <Magick++/Image.h>
#include <fstream>
#include <iostream>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string>
#include <vector>

using namespace std;
using namespace Magick;

class MagickStateOLD {
public:
  MagickStateOLD() {
    Magick::InitializeMagick(nullptr);
    MagickCore::SetMagickResourceLimit(MagickCore::MemoryResource, 1000000000);
    MagickCore::SetMagickResourceLimit(MagickCore::WidthResource, 2048);
    MagickCore::SetMagickResourceLimit(MagickCore::HeightResource, 2048);
  }
};

MagickStateOLD kMagickState2;

static int g_rv_mode = 0;

extern "C" void imagemagick_set_rv_mode(int mode) {
  if (mode < 0)
    mode = 0;
  if (mode > 2)
    mode = 2;
  g_rv_mode = mode;
}

static uint32_t fnv1a32_update(uint32_t hash, const char *data, size_t len) {
  const uint8_t *p = (const uint8_t *)data;
  for (size_t i = 0; i < len; ++i) {
    hash ^= (uint32_t)p[i];
    hash *= 16777619u;
  }
  return hash;
}

static int short_hash_u16(uint32_t hash) { return (int)((hash % 65535u) + 1u); }

static bool extract_im_file_line(const std::string &msg, std::string &out_path,
                                 int &out_line) {
  size_t at = msg.find("@ error/");
  if (at == std::string::npos)
    at = msg.find("@ warning/");
  if (at == std::string::npos)
    return false;

  // Start at the token after "@ ", so we keep the leading directory ("error").
  size_t start = at + 2;
  size_t end = msg.find_first_of(" \t\r\n", start);
  if (end == std::string::npos)
    end = msg.size();
  if (end <= start)
    return false;

  std::string token = msg.substr(start, end - start);
  // token example: "error/blob.c/BlobToImage/458"
  size_t first = token.find('/');
  if (first == std::string::npos)
    return false;
  size_t second = token.find('/', first + 1);
  if (second == std::string::npos)
    return false;
  size_t last = token.rfind('/');
  if (last == std::string::npos || last + 1 >= token.size())
    return false;

  out_path = token.substr(0, second);
  out_line = atoi(token.c_str() + last + 1);
  return (!out_path.empty() && out_line > 0);
}

static int encode_error_legacy(const char *str) {
  const std::string target(str);
  size_t idx = target.find("@ error");
  if (idx == std::string::npos) {
    idx = target.find("@ warning");
    if (idx == std::string::npos)
      return -1;
  }

  size_t value = 0;
  for (int i = idx + 8; i < target.length(); i++)
    value += (i - (idx + 8)) * str[i];

  return value;
}

static int encode_error(const char *str) {
  // rv_mode=0 is the binary flag mode: 0=success, 1=error.
  if (g_rv_mode == 0)
    return 1;

  const std::string msg(str);
  std::string path;
  int line = 0;
  if (!extract_im_file_line(msg, path, line)) {
    // Fallback: hash the whole message to keep the return value short and
    // deterministic even when we cannot extract a file/line token.
    uint32_t h = 2166136261u;
    h = fnv1a32_update(h, msg.data(), msg.size());
    return short_hash_u16(h);
  }

  uint32_t h = 2166136261u;
  h = fnv1a32_update(h, path.data(), path.size());
  if (g_rv_mode == 2) {
    char buf[32];
    int n = snprintf(buf, sizeof(buf), ":%d", line);
    if (n > 0)
      h = fnv1a32_update(h, buf, (size_t)n);
  }
  return short_hash_u16(h);
}

extern "C" int imagemagick_parse(const uint8_t *data, size_t size) {
  const Magick::Blob blob(data, size);
  InitializeMagick(".");
  Image image;
  try {
    image.read(blob);
  } catch (Exception &error_) {
    cout << "Caught exception on IM: " << error_.what() << endl;
    return encode_error(error_.what());
  }
  return 0;
}
