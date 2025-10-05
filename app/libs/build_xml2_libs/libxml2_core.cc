// Core implementation moved from libxml2_harness.cc
#include "../libxml.h"
#include <assert.h>
#include <ctype.h>
#include <dirent.h>
#include <fstream>
#include <functional>
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
#include <sstream>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <string>
#include <unistd.h>
#include <vector>

extern "C" int init_xml2_xmlparse(const uint8_t *data, size_t size) {
  std::string xmlData(reinterpret_cast<const char *>(data), size);
  LIBXML_TEST_VERSION
  xmlDocPtr doc =
      xmlReadMemory(xmlData.c_str(), xmlData.size(), "noname.xml", NULL, 0);
  if (doc == NULL) {
    return 2;
  }
  xmlNode *root = xmlDocGetRootElement(doc);
  if (root == NULL) {
    xmlFreeDoc(doc);
    return 3;
  }

  std::function<void(xmlNode *)> traverseElements = [&](xmlNode *element) {
    const char *elementName = reinterpret_cast<const char *>(element->name);
    const char *elementText =
        reinterpret_cast<const char *>(xmlNodeGetContent(element));
    if (elementText) {
      xmlFree((void *)elementText);
    }
    for (xmlAttr *attribute = element->properties; attribute;
         attribute = attribute->next) {
      const char *attributeValue = reinterpret_cast<const char *>(
          xmlNodeGetContent(attribute->children));
      if (attributeValue) {
        xmlFree((void *)attributeValue);
      }
    }
    for (xmlNode *child = element->children; child; child = child->next) {
      if (child->type == XML_ELEMENT_NODE) {
        traverseElements(child);
      }
    }
  };

  traverseElements(root);
  xmlFreeDoc(doc);
  xmlCleanupParser();
  return 0;
}

extern "C" int xml2_xmlparse(const uint8_t *data, size_t size) {
  FILE *tempFile = tmpfile();
  if (tempFile == NULL) {
    perror("tmpfile");
    return 1;
  }
  int oldStderrFd = dup(fileno(stderr));
  if (oldStderrFd == -1) {
    perror("dup");
    fclose(tempFile);
    return 1;
  }
  if (dup2(fileno(tempFile), fileno(stderr)) == -1) {
    perror("dup2");
    close(oldStderrFd);
    fclose(tempFile);
    return 1;
  }
  int ret = init_xml2_xmlparse(data, size);
  if (dup2(oldStderrFd, fileno(stderr)) == -1) {
    perror("dup2");
    close(oldStderrFd);
    fclose(tempFile);
    return 1;
  }
  close(oldStderrFd);
  fseek(tempFile, 0, SEEK_SET);
  std::ostringstream buffer;
  char ch;
  while ((ch = fgetc(tempFile)) != EOF) {
    buffer << ch;
  }
  fclose(tempFile);
  std::string output = buffer.str();
  std::cout << "Captured stderr: " << output << std::endl;
  if (ret != 0) {
    return 1;
  }
  if (output.find("error") != std::string::npos) {
    return 1;
  }
  return 0;
}
