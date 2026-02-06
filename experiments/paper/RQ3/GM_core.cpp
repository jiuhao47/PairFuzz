#include <Magick++.h>
#include <cstdint>
#include <dirent.h>
#include <fstream>
#include <iostream>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <string>
#include <vector>

#include "magick/describe.h"
// Core implementation extracted from GM_harness.cpp
using namespace std;

void deleteFilesWithPrefix(const char *directoryPath, const char *prefix) {
  DIR *dir;
  struct dirent *entry;

  // Open the directory
  dir = opendir(directoryPath);
  if (!dir) {
    perror("Error opening directory");
    exit(EXIT_FAILURE);
  }

  // Traverse the directory
  while ((entry = readdir(dir)) != NULL) {
    if (entry->d_type == DT_REG &&
        strncmp(entry->d_name, prefix, strlen(prefix)) == 0) {
      // Construct the full path of the file
      char filePath[PATH_MAX];
      snprintf(filePath, sizeof(filePath), "%s/%s", directoryPath,
               entry->d_name);

      // Attempt to delete the file
      if (remove(filePath) == 0) {
        printf("Deleted file: %s\n", filePath);
      } else {
        perror("Error deleting file");
      }
    }
  }

  // Close the directory
  closedir(dir);
}

class MagickStateOLD {
public:
  MagickStateOLD() {
    // printf("init at startup for oldgm\n");
    Magick::InitializeMagick(nullptr);
    MagickLib::SetMagickResourceLimit(MagickLib::MemoryResource, 1000000000);
    MagickLib::SetMagickResourceLimit(MagickLib::WidthResource, 2048);
    MagickLib::SetMagickResourceLimit(MagickLib::HeightResource, 2048);
  }
};

// Static initializer so this code is run once at startup.
MagickStateOLD kMagickState2;

static int g_rv_mode = 0;

extern "C" void GM_set_rv_mode(int mode) {
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

static int short_hash_u16(uint32_t hash) {
  // Map to 1..65535 so 0 can remain "success".
  return (int)((hash % 65535u) + 1u);
}

static bool extract_gm_file_line(const std::string &msg, std::string &out_path,
                                 int &out_line) {
  const char *marker = "reported by";
  const size_t marker_len = strlen(marker);
  size_t idx = msg.find(marker);
  if (idx == std::string::npos)
    return false;

  size_t start = idx + marker_len;
  while (start < msg.size() && msg[start] == ' ')
    ++start;
  if (start >= msg.size())
    return false;

  size_t end = msg.find_first_of(" (\t\r\n", start);
  if (end == std::string::npos)
    end = msg.size();
  if (end <= start)
    return false;

  std::string token = msg.substr(start, end - start);
  size_t colon = token.rfind(':');
  if (colon == std::string::npos || colon == 0 || colon + 1 >= token.size())
    return false;

  out_path.assign(token.data(), colon);
  out_line = atoi(token.c_str() + colon + 1);
  return (!out_path.empty() && out_line > 0);
}

static int encode_error_legacy(const char *str) {
  const std::string target(str);
  size_t idx = target.find("reported by");
  // no such report
  if (idx == std::string::npos) {
    return -1;
  }
  size_t funcIdx = target.rfind("(");
  size_t value = 0;
  for (int i = idx + 12; i < funcIdx - 1; i++) {
    value += (i - (idx + 12)) * str[i];
  }
  // return (value==0) ? 0 : 1;
  return value;
}

static int encode_error(const char *str) {
  // rv_mode=0 is the binary flag mode: 0=success, 1=error.
  if (g_rv_mode == 0)
    return 1;

  const std::string msg(str);
  std::string path;
  int line = 0;
  if (!extract_gm_file_line(msg, path, line)) {
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

extern "C" int GM_parse(const uint8_t *data, size_t size) {
  // const Magick::Blob blob(data, size);
  Magick::InitializeMagick(".");
  Magick::Image image;
  const Magick::Blob blob(data, size);
  const char *directoryPath = "/tmp";
  const char *prefix = "magick-";

  try {
    // printf("gm function:%p\n", InitializeMagick);
    deleteFilesWithPrefix(directoryPath, prefix);
    image.read(blob);
    deleteFilesWithPrefix(directoryPath, prefix);

  } catch (Magick::Exception &error_) {
    const int rv = encode_error(error_.what());
    cout << "Caught exception on old gm: " << error_.what() << ":" << rv
         << endl;
    return rv;
  }
  return 0;
}
