#include "../libxml.h"
// #include <fuzzer/FuzzedDataProvider.h>
#include <assert.h>
#include <ctype.h>
#include <dirent.h>
#include <fstream>
#include <functional> // 添加此行以包含 std::function
#include <iostream>
#include <libxml2/libxml/HTMLparser.h>
#include <libxml2/libxml/SAX2.h>
#include <libxml2/libxml/catalog.h>
#include <libxml2/libxml/encoding.h>
#include <libxml2/libxml/entities.h>
#include <libxml2/libxml/parser.h>
#include <libxml2/libxml/parserInternals.h>
#include <libxml2/libxml/tree.h>
#include <libxml2/libxml/uri.h>
#include <libxml2/libxml/valid.h>
#include <libxml2/libxml/xmlIO.h>
#include <libxml2/libxml/xmlerror.h>
#include <libxml2/libxml/xmlmemory.h>
#include <limits.h>
#include <sstream> // 添加此行以包含 std::ostringstream
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <string>
#include <unistd.h> // 添加此行以包含 dup、dup2 和 close 函数
#include <vector>

extern "C" int init_xml2_xmlparse(const uint8_t *data, size_t size) {
  // Convert the input data to a string
  std::string xmlData(reinterpret_cast<const char *>(data), size);

  // Initialize the library and check potential ABI mismatches
  LIBXML_TEST_VERSION

  // Parse the XML data
  xmlDocPtr doc =
      xmlReadMemory(xmlData.c_str(), xmlData.size(), "noname.xml", NULL, 0);
  if (doc == NULL) {
    return 2; // If parsing fails, return early
  }
// Thin harness wrapper for libxml2 core implementation.
#include <stddef.h>
#include <stdint.h>

// Include the core implementation so the harness is a single-file artifact
// that exports the expected C entrypoint used by diffRay.
#include "libxml2_core.cc"

  extern "C" int diffRayTestOneInput_1(const uint8_t *data, size_t size) {
    return xml2_xmlparse(data, size);
  }