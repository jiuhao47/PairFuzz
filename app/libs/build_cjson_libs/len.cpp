#include <stdint.h>
#include <stddef.h>
#include <stdio.h>  // For printf
#include <stdlib.h>
#include <string.h>
#include "cJSON.h"
#include <assert.h>
#include <iostream>
#include <fstream>
#include <string>
#include <vector>

//这个测试len无意义哈，是我自己memcpy的
extern int cjson_parse(const uint8_t *Data, size_t Size) {

    if (Size==0) {
        return 100;
    }

    unsigned char *buf = (unsigned char *) malloc(Size+1);
    buf[Size] = 0;
    memcpy(buf, Data, Size);

    int ret = 1;
    cJSON *root = NULL;
    root = cJSON_Parse((char *)buf);

    if (root !=NULL &&  root->type == 64 ) // json object
    {
        ret = 0;
        cJSON_Delete(root);
    }else{
        if (root!=NULL) {
            cJSON_Delete(root);
        }
    }

    free(buf);

    return ret;
}

int main(int argc, char **argv) {
    std::ifstream in(argv[1]);
    in.seekg(0, in.end);
    size_t length = in.tellg();
    in.seekg(0, in.beg);
    std::vector<char> bytes(length);
    in.read(bytes.data(), bytes.size());
  
    int ret = cjson_parse(reinterpret_cast<const uint8_t *>(bytes.data()),
                         -10086);//bytes.size());
  
    printf("ret: %d\n", ret);
  
    return 0;
  }