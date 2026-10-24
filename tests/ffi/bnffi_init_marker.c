// Test library with constructor initializer to verify AC12.
#define _CRT_SECURE_NO_WARNINGS
#include <stdio.h>

#if defined(_WIN32)
#include <windows.h>
BOOL WINAPI DllMain(HINSTANCE hinstDLL, DWORD fdwReason, LPVOID lpvReserved) {
    if (fdwReason == DLL_PROCESS_ATTACH) {
        FILE *f = fopen("bnffi_init_marker.tmp", "w");
        if (f) {
            fputs("initialized", f);
            fclose(f);
        }
    }
    return TRUE;
}
#else
__attribute__((constructor))
static void on_init(void) {
    FILE *f = fopen("bnffi_init_marker.tmp", "w");
    if (f) {
        fputs("initialized", f);
        fclose(f);
    }
}
#endif

#if defined(_WIN32)
__declspec(dllexport)
#else
__attribute__((visibility("default")))
#endif
int get_marker_value(void) {
    return 42;
}
