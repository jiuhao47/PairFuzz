// Do not include the implementation .cpp files here: some of those files in the
// repository contain non-standard wrappers/markers that break compilation when
// included. Instead, declare the external C entry points provided by those
// implementations and link them at build time.

#include <cerrno>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iostream>
#include <vector>

// External verifier functions (provided by the wolfssl/openssl implementation
// files in the project or by linking corresponding libraries). Use uint8_t/
// uint32_t to match their definitions.
#include "wolfssl_x509_core.cpp"
// Load a DER file into a vector. Returns true on success.
static bool load_der_cert(const char *filename,
                          std::vector<unsigned char> &out) {
  if (!filename) {
    std::cerr << "load_der_cert: filename is null\n";
    return false;
  }

  std::ifstream ifs(filename, std::ios::binary | std::ios::ate);
  if (!ifs) {
    std::cerr << "Error opening certificate file '" << filename
              << "': " << std::strerror(errno) << "\n";
    return false;
  }

  std::streamsize size = ifs.tellg();
  if (size <= 0) {
    std::cerr << "Empty or invalid file: '" << filename << "'\n";
    return false;
  }

  out.resize(static_cast<size_t>(size));
  ifs.seekg(0, std::ios::beg);
  if (!ifs.read(reinterpret_cast<char *>(out.data()), size)) {
    std::cerr << "Failed to read file '" << filename << "'\n";
    out.clear();
    return false;
  }

  return true;
}

int main(int argc, char *argv[]) {
  if (argc != 2) {
    std::cerr << "usage: " << (argv && argv[0] ? argv[0] : "ssl_verify")
              << " [filename]\n";
    return 2;
  }

  const char *certificate_file = argv[1];
  std::vector<unsigned char> buffer;
  if (!load_der_cert(certificate_file, buffer)) {
    return 3;
  }

  uint32_t file_size = static_cast<uint32_t>(buffer.size());

  // Call the verification functions.
  int wolfssl_verify_result = verify_wolfssl(buffer.data(), file_size);
  std::cout << "Result = " << wolfssl_verify_result << "\n";

  return 0;
}
