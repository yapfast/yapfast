//! Android `cdylib` entrypoint (`libyapfast.so`) for Android NativeActivity.

#[cfg(target_os = "android")]
mod android_exports {
    use yapfast::android_activity::AndroidApp;
    use yapfast::jni::{
        EnvUnowned,
        objects::{JClass, JObjectArray, JString},
        sys::{jboolean, jlong},
    };

    #[unsafe(no_mangle)]
    pub fn android_main(app: AndroidApp) {
        yapfast::android_main(app);
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_yapfast_YapfastActivity_nativeOnFilesPicked<'local>(
        env: EnvUnowned<'local>,
        _class: JClass<'local>,
        request_id: jlong,
        paths: JObjectArray<'local, JString<'local>>,
    ) {
        yapfast::android::jni_on_files_picked(env, request_id, paths);
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_yapfast_YapfastActivity_nativeOnSaveTarget<'local>(
        env: EnvUnowned<'local>,
        _class: JClass<'local>,
        request_id: jlong,
        path: JString<'local>,
    ) {
        yapfast::android::jni_on_save_target(env, request_id, path);
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_yapfast_YapfastActivity_nativeOnOpenRoom<'local>(
        env: EnvUnowned<'local>,
        _class: JClass<'local>,
        room: JString<'local>,
    ) {
        yapfast::android::jni_on_open_room(env, room);
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_yapfast_YapfastActivity_nativeOnDeepLink<'local>(
        env: EnvUnowned<'local>,
        _class: JClass<'local>,
        link: JString<'local>,
    ) {
        yapfast::android::jni_on_deep_link(env, link);
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_yapfast_YapfastActivity_nativeOnPermissionResult<
        'local,
    >(
        _env: EnvUnowned<'local>,
        _class: JClass<'local>,
        request_id: jlong,
        granted: jboolean,
    ) {
        yapfast::android::jni_on_permission_result(request_id, granted);
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_github_yapfast_YapfastActivity_nativeOnDisconnectCall<'local>(
        _env: EnvUnowned<'local>,
        _class: JClass<'local>,
    ) {
        yapfast::android::jni_on_disconnect_call();
    }
}
