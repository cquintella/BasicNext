// Basic Next BNFfi test dynamic library functions.
#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
#define EXPORT __declspec(dllexport)
#else
#define EXPORT __attribute__((visibility("default")))
#endif

EXPORT int8_t test_int8(int8_t a, int8_t b) {
    return (int8_t)(a + b);
}

EXPORT uint8_t test_uint8(uint8_t a, uint8_t b) {
    return (uint8_t)(a + b);
}

EXPORT int16_t test_int16(int16_t a, int16_t b) {
    return (int16_t)(a + b);
}

EXPORT uint16_t test_uint16(uint16_t a, uint16_t b) {
    return (uint16_t)(a + b);
}

EXPORT int32_t test_int32(int32_t a, int32_t b) {
    return a + b;
}

EXPORT uint32_t test_uint32(uint32_t a, uint32_t b) {
    return a + b;
}

EXPORT int64_t test_int64(int64_t a, int64_t b) {
    return a + b;
}

EXPORT uint64_t test_uint64(uint64_t a, uint64_t b) {
    return a + b;
}

EXPORT int test_int(int a, int b) {
    return a + b;
}

EXPORT unsigned int test_uint(unsigned int a, unsigned int b) {
    return a + b;
}

EXPORT long test_long(long a, long b) {
    return a + b;
}

EXPORT unsigned long test_ulong(unsigned long a, unsigned long b) {
    return a + b;
}

EXPORT size_t test_sizet(size_t a, size_t b) {
    return a + b;
}

EXPORT float test_float(float a, float b) {
    return a + b;
}

EXPORT double test_double(double a, double b) {
    return a * b;
}

EXPORT bool test_bool(bool a, bool b) {
    return a && b;
}

EXPORT int32_t test_strlen(const char *s) {
    return s ? (int32_t)strlen(s) : -1;
}

EXPORT void test_void_increment(int32_t *counter) {
    if (counter) {
        (*counter)++;
    }
}

EXPORT void test_double_array_scale(double *arr, int32_t len, double factor) {
    if (arr) {
        for (int32_t i = 0; i < len; ++i) {
            arr[i] *= factor;
        }
    }
}

EXPORT void *test_alloc_bytes(size_t size) {
    return malloc(size);
}

EXPORT void test_free_bytes(void *ptr) {
    free(ptr);
}

EXPORT void *test_return_null(void) {
    return NULL;
}

EXPORT const char *test_static_string(void) {
    return "hello from C";
}
