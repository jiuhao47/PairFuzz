#include <Magick++.h>
#include <cstdint>
#include <dirent.h>
#include <fstream>
#include <iostream>
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

int basic_encode_error(const char *str) {
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
    cout << "Caught exception on old gm: " << error_.what() << ":"
         << basic_encode_error(error_.what()) << endl;
    return basic_encode_error(error_.what());
  }
  return 0;
}
