//! Clipboard on Android through JNI, with no Java code: the `Context` that
//! android-activity 0.6.1 registers with `ndk-context` (the `Application`)
//! gives `ClipboardManager`.
use jni::objects::{JObject, JString, JValue};
use jni::vm::JavaVM;
use jni::{jni_sig, jni_str};

fn with_context<T>(
    f: impl FnOnce(&mut jni::Env, &JObject) -> jni::errors::Result<T>,
) -> jni::errors::Result<T> {
    let ctx = ndk_context::android_context();
    // SAFETY: android-activity initialised ndk-context with a valid JavaVM
    // and a global reference to the Application before android_main.
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) };
    let context = ctx.context() as jni::sys::jobject;
    vm.attach_current_thread(|env| {
        // SAFETY: a global reference that android-activity never deletes.
        let context = unsafe { env.as_cast_raw::<JObject>(&context)? };
        f(env, &context)
    })
}

fn clipboard_manager<'local>(
    env: &mut jni::Env<'local>,
    context: &JObject,
) -> jni::errors::Result<JObject<'local>> {
    let name = env.new_string("clipboard")?; // Context.CLIPBOARD_SERVICE
    env.call_method(
        context,
        jni_str!("getSystemService"),
        jni_sig!((java.lang.String) -> java.lang.Object),
        &[JValue::Object(&name)],
    )?
    .l()
}

pub fn write(text: &str) -> jni::errors::Result<()> {
    with_context(|env, context| {
        let manager = clipboard_manager(env, context)?;
        let label = env.new_string("Tawara")?;
        let value = env.new_string(text)?;
        let clip = env
            .call_static_method(
                jni_str!("android/content/ClipData"),
                jni_str!("newPlainText"),
                jni_sig!((java.lang.CharSequence, java.lang.CharSequence) -> android.content.ClipData),
                &[JValue::Object(&label), JValue::Object(&value)],
            )?
            .l()?;
        env.call_method(
            &manager,
            jni_str!("setPrimaryClip"),
            jni_sig!((android.content.ClipData) -> void),
            &[JValue::Object(&clip)],
        )?;
        Ok(())
    })
}

pub fn read() -> jni::errors::Result<Option<String>> {
    with_context(|env, context| {
        let manager = clipboard_manager(env, context)?;
        let clip = env
            .call_method(
                &manager,
                jni_str!("getPrimaryClip"),
                jni_sig!(() -> android.content.ClipData),
                &[],
            )?
            .l()?;
        if clip.is_null() {
            return Ok(None);
        }
        let item = env
            .call_method(
                &clip,
                jni_str!("getItemAt"),
                jni_sig!((jint) -> android.content.ClipData::Item),
                &[JValue::Int(0)],
            )?
            .l()?;
        let text = env
            .call_method(
                &item,
                jni_str!("coerceToText"),
                jni_sig!((android.content.Context) -> java.lang.CharSequence),
                &[JValue::Object(context)],
            )?
            .l()?;
        let text = env
            .call_method(
                &text,
                jni_str!("toString"),
                jni_sig!(() -> java.lang.String),
                &[],
            )?
            .l()?;
        let text = JString::cast_local(env, text)?;
        Ok(Some(text.try_to_string(env)?))
    })
}

/// What Android 12+ does itself for a root launcher activity on Back: move
/// the task to the background, keeping the Activity (and winit's event loop)
/// alive.
pub fn move_task_to_back(
    app: &winit::platform::android::activity::AndroidApp,
) -> jni::errors::Result<()> {
    // SAFETY: both pointers stay valid while `app` is alive (android-activity
    // documents `vm_as_ptr` and `activity_as_ptr` this way).
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) };
    let activity = app.activity_as_ptr() as jni::sys::jobject;
    vm.attach_current_thread(|env| {
        let activity = unsafe { env.as_cast_raw::<JObject>(&activity)? };
        env.call_method(
            &activity,
            jni_str!("moveTaskToBack"),
            jni_sig!((jboolean) -> jboolean),
            &[JValue::Bool(true)],
        )?;
        Ok(())
    })
}
