// Core implementation for ImageMagick harness
#include <Magick++.h>
#include <Magick++/Blob.h>
#include <Magick++/Image.h>
#include <fstream>
#include <iostream>
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

int basic_encode_error(const char *str) {
  const std::string target(str);
  size_t            idx = target.find("@ error");
  if (idx == std::string::npos) {
    idx = target.find("@ warning");
    if (idx == std::string::npos) return -1;
  }

  size_t value = 0;
  for (int i = idx + 8; i < target.length(); i++)
    value += (i - (idx + 8)) * str[i];

  return value;
}

extern "C" int imagemagick_parse(const uint8_t *data, size_t size) {
  const Magick::Blob blob(data, size);
  InitializeMagick(".");
  Image image;
  try {
    image.read(blob);
  } catch (Exception &error_) {
    // cout << "Caught exception on IM: " << error_.what() << endl;
    return basic_encode_error(error_.what());
  }
  return 0;
}
