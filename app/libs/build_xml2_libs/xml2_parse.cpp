#include "../libxml.h"
//#include <fuzzer/FuzzedDataProvider.h>
#include <string>
#include <vector>
#include <functional> // 添加此行以包含 std::function
#include <stdio.h>
#include <assert.h>
#include <stdint.h>
#include <iostream>
#include <fstream>
#include <sstream> // 添加此行以包含 std::ostringstream
#include <string>
#include <vector>
#include <stdlib.h>
#include <string.h>
#include <dirent.h>
#include <limits.h>
#include <stdarg.h>
#include <stddef.h>
#include <ctype.h>
#include <unistd.h> // 添加此行以包含 dup、dup2 和 close 函数
#include <libxml/parser.h>
#include <libxml/xmlmemory.h>
#include <libxml/tree.h>
#include <libxml/parserInternals.h>
#include <libxml/valid.h>
#include <libxml/entities.h>
#include <libxml/xmlerror.h>
#include <libxml/encoding.h>
#include <libxml/xmlIO.h>
#include <libxml/uri.h>
#include <libxml/SAX2.h>
#include <libxml/HTMLparser.h>
#include <libxml/catalog.h>

extern "C" int init_xml2_xmlparse(const uint8_t *data, size_t size) {
    // Convert the input data to a string
    std::string xmlData(reinterpret_cast<const char*>(data), size);

    // Initialize the library and check potential ABI mismatches
    LIBXML_TEST_VERSION

    // Parse the XML data
    xmlDocPtr doc = xmlReadMemory(xmlData.c_str(), xmlData.size(), "noname.xml", NULL, 0);
    if (doc == NULL) {
        return 2; // If parsing fails, return early
    }

    // Get the root element
    xmlNode *root = xmlDocGetRootElement(doc);
    if (root == NULL) {
        xmlFreeDoc(doc);
        return 3; // If no root element, return early
    }

    // Function to recursively traverse XML elements
    std::function<void(xmlNode*)> traverseElements = [&](xmlNode* element) {
        // Process the current element
        const char* elementName = reinterpret_cast<const char*>(element->name);
        const char* elementText = reinterpret_cast<const char*>(xmlNodeGetContent(element));
        if (elementName) {
            // Print element name (for debugging purposes)
            //printf("Element: %s\n", elementName);
        }
        if (elementText) {
            // Print element text (for debugging purposes)
            //printf("Text: %s\n", elementText);
            xmlFree((void*)elementText); // Free the text content
        }

        // Process attributes
        for (xmlAttr* attribute = element->properties; attribute; attribute = attribute->next) {
            const char* attributeName = reinterpret_cast<const char*>(attribute->name);
            const char* attributeValue = reinterpret_cast<const char*>(xmlNodeGetContent(attribute->children));
            if (attributeName && attributeValue) {
                // Print attribute name and value (for debugging purposes)
                //printf("Attribute: %s = %s\n", attributeName, attributeValue);
                xmlFree((void*)attributeValue); // Free the attribute value
            }
        }

        // Recursively process child elements
        for (xmlNode* child = element->children; child; child = child->next) {
            if (child->type == XML_ELEMENT_NODE) {
                traverseElements(child);
            }
        }
    };

    // Start traversing from the root element
    traverseElements(root);

    // Free the document
    xmlFreeDoc(doc);

    // Cleanup function for the XML library
    xmlCleanupParser();

    return 0;
}

extern "C" int xml2_xmlparse(const uint8_t *data, size_t size) {
    // 创建一个临时文件来捕获标准错误输出
    FILE* tempFile = tmpfile();
    if (tempFile == NULL) {
        perror("tmpfile");
        return 1;
    }

    // 保存原始的标准错误文件描述符
    int oldStderrFd = dup(fileno(stderr));
    if (oldStderrFd == -1) {
        perror("dup");
        fclose(tempFile);
        return 1;
    }

    // 将标准错误输出重定向到临时文件
    if (dup2(fileno(tempFile), fileno(stderr)) == -1) {
        perror("dup2");
        close(oldStderrFd);
        fclose(tempFile);
        return 1;
    }

    // 调用目标函数
    int ret = init_xml2_xmlparse(data, size);

    // 恢复标准错误输出
    if (dup2(oldStderrFd, fileno(stderr)) == -1) {
        perror("dup2");
        close(oldStderrFd);
        fclose(tempFile);
        return 1;
    }
    close(oldStderrFd);

    // 将临时文件的内容读取到字符串中
    fseek(tempFile, 0, SEEK_SET);
    std::ostringstream buffer;
    char ch;
    while ((ch = fgetc(tempFile)) != EOF) {
        buffer << ch;
    }
    fclose(tempFile);

    // 获取捕获的输出
    std::string output = buffer.str();

    // 打印捕获的输出
    std::cout << "Captured stderr: " << output << std::endl;
    if(ret != 0) {return 1;}
    // 检查输出中是否包含 "error"
    if (output.find("error") != std::string::npos) {
        return 1;
    }

    return 0;
}

int main(int argc, char **argv) {
    std::ifstream in(argv[1]);
    in.seekg(0, in.end);
    size_t length = in.tellg();
    in.seekg(0, in.beg);
    std::vector<char> bytes(length);
    in.read(bytes.data(), bytes.size());

    int ret = xml2_xmlparse(reinterpret_cast<const uint8_t *>(bytes.data()), bytes.size());

    printf("ret: %d\n", ret);

    return ret;
}