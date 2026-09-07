// Target metadata runner, linked with the whole application archive by generate.py.
#include "runtime.h"
#include <stdio.h>
#include <string.h>

extern BridgeResult bridgerton_generate_v1(const unsigned char *, size_t);
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    BridgeResult result = bridgerton_generate_v1((const unsigned char *)argv[1], strlen(argv[1]));
    if (result.status) fwrite(result.data.data, 1, result.data.len, stderr);
    bridgerton_buffer_free(result.data);
    return result.status ? 1 : 0;
}
