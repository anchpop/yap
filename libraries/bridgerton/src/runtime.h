#pragma once
#include <stdint.h>
#include <stddef.h>
typedef struct { uint8_t *data; size_t len; } BridgeBuffer;
typedef struct { const uint8_t *data; size_t len; } BridgeBytes;
typedef struct { const void *handle; uint32_t value; uint32_t status; BridgeBuffer data; } BridgeResult;
typedef struct { size_t context; uint8_t (*invoke)(size_t, BridgeResult); void (*release)(size_t); } BridgeHostCallback;
BridgeResult bridgerton_task_poll(const void *handle, void (*wake)(uint64_t), uint64_t context);
BridgeResult bridgerton_task_free(const void *handle);
void bridgerton_buffer_free(BridgeBuffer buffer);

uint8_t bridgerton_abi_v1_matches(const uint8_t *expected, size_t len);

BridgeResult bridgerton_sequence_next(const void *handle);
BridgeResult bridgerton_sequence_free(const void *handle);
