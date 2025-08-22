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
    // printf("init at startup for oldgm\n");
    Magick::InitializeMagick(nullptr);
    MagickCore::SetMagickResourceLimit(MagickCore::MemoryResource, 1000000000);
    MagickCore::SetMagickResourceLimit(MagickCore::WidthResource, 2048);
    MagickCore::SetMagickResourceLimit(MagickCore::HeightResource, 2048);
  }
};

// Static initializer so this code is run once at startup.
MagickStateOLD kMagickState2;

int basic_encode_error(const char *str) {
  const std::string target(str);
  // size_t i = target.find("@ warning");
  // if (i!=std::string::npos) return 0;
  size_t idx = target.find("@ error");

  // no such report, so we directly return
  if (idx == std::string::npos) {
    idx = target.find("@ warning");
    if (idx == std::string::npos)
      return -1;
  }

  size_t value = 0;
  // encode the exception to an integer
  for (int i = idx + 8; i < target.length(); i++)
    value += (i - (idx + 8)) * str[i];

  // return (value==0) ? 0 : 1;
  return value;
}

extern "C" int imagemagick_parse(const uint8_t *data, size_t size) {
  const Magick::Blob blob(data, size);
  InitializeMagick(".");
  Image image;
  try {
    image.read(blob);
  } catch (Exception &error_) {
    // return 1;
    cout << "Caught exception on IM: " << error_.what() << endl;
    return basic_encode_error(error_.what());
  }
  return 0;
}

int main(int argc, char *argv[]) {
  if (argc != 2) {
    printf("usage: %s [filename]", argv[0]);
    return 0;
  }
  std::ifstream in(argv[1]);
  in.seekg(0, in.end);
  size_t length = in.tellg();
  in.seekg(0, in.beg);
  std::vector<char> bytes(length);
  in.read(bytes.data(), bytes.size());
  imagemagick_parse(reinterpret_cast<const uint8_t *>(bytes.data()),
                    bytes.size());

  return 0;
}
