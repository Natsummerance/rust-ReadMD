#include <napi/native_api.h>
#include <string>
extern "C" {
char* readmd_kernel_start(const char*, const char*);
void readmd_kernel_stop();
void readmd_kernel_free(char*);
}
static napi_value Start(napi_env env, napi_callback_info info) {
    size_t argc = 2;
    napi_value argv[2];
    napi_get_cb_info(env, info, &argc, argv, nullptr, nullptr);
    if (argc != 2) { napi_throw_type_error(env, nullptr, "Two directories required"); return nullptr; }
    std::string paths[2];
    for (size_t i=0; i<2; ++i) {
        size_t size=0;
        if (napi_get_value_string_utf8(env, argv[i], nullptr, 0, &size) != napi_ok || size>4096) {
            napi_throw_type_error(env, nullptr, "Invalid directory"); return nullptr;
        }
        paths[i].resize(size+1);
        napi_get_value_string_utf8(env, argv[i], paths[i].data(), size+1, &size);
        paths[i].resize(size);
    }
    char* raw=readmd_kernel_start(paths[0].c_str(), paths[1].c_str());
    napi_value result;
    napi_create_string_utf8(env, raw, NAPI_AUTO_LENGTH, &result);
    readmd_kernel_free(raw);
    return result;
}
static napi_value Stop(napi_env env, napi_callback_info) {
    readmd_kernel_stop(); napi_value result; napi_get_undefined(env, &result); return result;
}
static napi_value Init(napi_env env, napi_value exports) {
    napi_property_descriptor props[] = {
        {"start", nullptr, Start, nullptr, nullptr, nullptr, napi_default, nullptr},
        {"stop", nullptr, Stop, nullptr, nullptr, nullptr, napi_default, nullptr}
    };
    napi_define_properties(env, exports, 2, props); return exports;
}
static napi_module module = {1, 0, nullptr, Init, "readmd_bridge", nullptr, {0}};
extern "C" __attribute__((constructor)) void RegisterReadMD() { napi_module_register(&module); }
