#include <stdio.h>
#include <expat.h>
#include <assert.h>
#include <stdint.h>
#include <iostream>
#include <fstream>
#include <string>
#include <vector>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <dirent.h>
#ifdef XML_LARGE_SIZE
#  define XML_FMT_INT_MOD "ll"
#else
#  define XML_FMT_INT_MOD "l"
#endif

#ifdef XML_UNICODE_WCHAR_T
#  define XML_FMT_STR "ls"
#else
#  define XML_FMT_STR "s"
#endif


static void XMLCALL
startElement(void *userData, const XML_Char *name, const XML_Char **atts) {
  int i;
  int *const depthPtr = (int *)userData;
  (void)atts;

  for (i = 0; i < *depthPtr; i++)
    putchar('\t');
//   printf("%" XML_FMT_STR "\n", name);
  *depthPtr += 1;
}

static void XMLCALL
endElement(void *userData, const XML_Char *name) {
  int *const depthPtr = (int *)userData;
  (void)name;

  *depthPtr -= 1;
}

extern "C" int expat_xmlparse(const uint8_t *data, size_t size) {
  XML_Parser parser = XML_ParserCreateNS(NULL, '!');
  int depth = 0;
  int ret = 0;
  if (!parser) {
    fprintf(stderr, "Couldn't allocate memory for parser\n");
    return 0;
  }
  XML_SetParamEntityParsing(parser,XML_PARAM_ENTITY_PARSING_ALWAYS);
  XML_SetUserData(parser, &depth);
  XML_SetElementHandler(parser, startElement, endElement);

  if (XML_Parse(parser, (const char *)data, (int)size, 1) == XML_STATUS_ERROR) {
    fprintf(stderr,
            "Parse error at line %" XML_FMT_INT_MOD "u:\n%" XML_FMT_STR "\n",
            XML_GetCurrentLineNumber(parser),
            XML_ErrorString(XML_GetErrorCode(parser)));
    //printf("error\n");
    ret = 1;
  }

  XML_ParserFree(parser);
  return (ret==0)?0:1;
}

int main(int argc, char **argv) {
    std::ifstream in(argv[1]);
    in.seekg(0, in.end);
    size_t length = in.tellg();
    in.seekg(0, in.beg);
    std::vector<char> bytes(length);
    in.read(bytes.data(), bytes.size());
  
    int ret = expat_xmlparse(reinterpret_cast<const uint8_t *>(bytes.data()),
                         bytes.size());
  
    printf("ret: %d\n", ret);
  
    return 0;
  }