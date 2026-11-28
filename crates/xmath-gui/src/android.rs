//! Android activity and the bridge to the system keyboard, document picker and print service.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use egui::{Context, Event, ImeEvent, Key, Modifiers};
use jni::errors::Result as JniResult;
use jni::objects::{JObject, JObjectArray, JString};
use jni::refs::{Global, IntoAuto};
use jni::strings::JNIStr;
use jni::{jni_sig, jni_str, Env, JValue, JavaVM};
use winit::platform::android::activity::AndroidApp;

use crate::{app, fonts};

static REPAINT: Mutex<Option<Context>> = Mutex::new(None);

#[no_mangle]
pub extern "system" fn Java_io_github_t1murjan_xmath_XmathActivity_wake(
    _: *mut jni::sys::JNIEnv,
    _: jni::sys::jclass,
) {
    if let Some(ctx) = REPAINT.lock().unwrap().as_ref() {
        ctx.request_repaint();
    }
}

#[no_mangle]
fn android_main(activity: AndroidApp) {
    let options = eframe::NativeOptions {
        android_app: Some(activity.clone()),
        persistence_path: activity
            .internal_data_path()
            .map(|p| p.join("xmath-ui.ron")),
        ..Default::default()
    };
    let result = eframe::run_native(
        "xmath",
        options,
        Box::new(move |cc| {
            let bold = fonts::setup_fonts(&cc.egui_ctx);
            app::setup_style(&cc.egui_ctx);
            *REPAINT.lock().unwrap() = Some(cc.egui_ctx.clone());
            let mut app = app::App::new(cc, [], bold);
            app.set_android(Bridge {
                vm: unsafe { JavaVM::from_raw(activity.vm_as_ptr().cast()) },
                activity,
                ime_focus: None,
            });
            Ok(Box::new(app))
        }),
    );
    *REPAINT.lock().unwrap() = None;
    if let Err(error) = result {
        eprintln!("xmath: {error}");
    }
}

pub(crate) struct Bridge {
    vm: JavaVM,
    activity: AndroidApp,
    ime_focus: Option<egui::Id>,
}

impl Bridge {
    pub(crate) fn reserve_system_insets(&self, ctx: &Context) {
        let area = self.activity.content_rect();
        if area.right <= area.left || area.bottom <= area.top {
            return;
        }
        let scale = ctx.pixels_per_point();
        let screen = ctx.screen_rect();
        let frame = egui::Frame::NONE.fill(app::PANEL);
        let top = area.top as f32 / scale;
        let bottom = screen.height() - area.bottom as f32 / scale;
        let left = area.left as f32 / scale;
        let right = screen.width() - area.right as f32 / scale;
        for (panel, height) in [
            (egui::TopBottomPanel::top("android-status"), top),
            (egui::TopBottomPanel::bottom("android-navigation"), bottom),
        ] {
            if height > 0.0 {
                panel.exact_height(height).frame(frame).show(ctx, |_| {});
            }
        }
        for (panel, width) in [
            (egui::SidePanel::left("android-left"), left),
            (egui::SidePanel::right("android-right"), right),
        ] {
            if width > 0.0 {
                panel
                    .resizable(false)
                    .exact_width(width)
                    .frame(frame)
                    .show(ctx, |_| {});
            }
        }
    }

    fn call<T>(
        &self,
        f: impl FnOnce(&mut Env<'_>, &JObject<'_>) -> JniResult<T>,
    ) -> Result<T, String> {
        self.vm
            .attach_current_thread(|env| {
                let raw = self.activity.activity_as_ptr().cast();
                // AndroidApp keeps this global reference alive for the duration of the call.
                let activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw)? };
                f(env, activity.as_ref())
            })
            .map_err(|e: jni::errors::Error| e.to_string())
    }

    fn strings(env: &mut Env<'_>, result: JObject<'_>) -> JniResult<Vec<String>> {
        if result.is_null() {
            return Ok(Vec::new());
        }
        let array = env.cast_local::<JObjectArray<JString>>(result)?;
        let len = array.len(env)?;
        let mut strings = Vec::with_capacity(len);
        for index in 0..len {
            let item = array.get_element(env, index)?.auto();
            strings.push(item.try_to_string(env)?);
        }
        Ok(strings)
    }

    pub(crate) fn pick(&self, kind: i32, name: &str) -> Result<(), String> {
        self.call(|env, activity| {
            let name = env.new_string(name)?;
            env.call_method(
                activity,
                jni_str!("pickDocument"),
                jni_sig!((i32, JString) -> ()),
                &[JValue::Int(kind), JValue::Object(&name)],
            )?;
            Ok(())
        })
    }

    pub(crate) fn take_document(&self) -> Option<Result<Option<PathBuf>, String>> {
        match self.take_file_result(jni_str!("takeDocumentResult")).as_deref() {
            Ok([]) => None,
            Ok([status, value]) if status == "ok" => Some(Ok(Some(PathBuf::from(value)))),
            Ok([status, _]) if status == "cancel" => Some(Ok(None)),
            Ok([_, error]) | Err(error) => Some(Err(error.clone())),
            _ => Some(Err("Неожиданный ответ системы выбора файлов".into())),
        }
    }

    pub(crate) fn take_external_document(&self) -> Option<Result<(PathBuf, bool), String>> {
        match self.take_file_result(jni_str!("takeExternalDocument")).as_deref() {
            Ok([]) => None,
            Ok([status, value]) if status == "ok" || status == "readonly" => {
                Some(Ok((PathBuf::from(value), status == "ok")))
            }
            Ok([_, error]) | Err(error) => Some(Err(error.clone())),
            _ => Some(Err("Неожиданный ответ при открытии файла".into())),
        }
    }

    fn take_file_result(&self, method: &JNIStr) -> Result<Vec<String>, String> {
        self.call(|env, activity| {
            let value = env
                .call_method(
                    activity,
                    method,
                    jni_sig!("()[Ljava/lang/String;"),
                    &[],
                )?
                .l()?;
            Self::strings(env, value)
        })
    }

    pub(crate) fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        self.call(|env, activity| {
            let path = env.new_string(path.to_string_lossy())?;
            let bytes = env.byte_array_from_slice(bytes)?;
            let value = env
                .call_method(
                    activity,
                    jni_str!("writeDocument"),
                    jni_sig!("(Ljava/lang/String;[B)Ljava/lang/String;"),
                    &[JValue::Object(&path), JValue::Object(&bytes)],
                )?
                .l()?;
            if value.is_null() {
                Ok(None)
            } else {
                Ok(Some(env.cast_local::<JString>(value)?.try_to_string(env)?))
            }
        })?
        .map_or(Ok(()), Err)
    }

    pub(crate) fn print(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        self.call(|env, activity| {
            let name = env.new_string(name)?;
            let bytes = env.byte_array_from_slice(bytes)?;
            env.call_method(
                activity,
                jni_str!("printPdf"),
                jni_sig!("(Ljava/lang/String;[B)V"),
                &[JValue::Object(&name), JValue::Object(&bytes)],
            )?;
            Ok(())
        })
    }

    pub(crate) fn inject_input(&self, ctx: &Context) {
        let result = self.call(|env, activity| {
            let value = env
                .call_method(
                    activity,
                    jni_str!("takeInput"),
                    jni_sig!("()[Ljava/lang/String;"),
                    &[],
                )?
                .l()?;
            Self::strings(env, value)
        });
        if let Ok(items) = result {
            ctx.input_mut(|input| {
                for item in items {
                    let key = match item.as_str() {
                        "backspace" => Key::Backspace,
                        "delete" => Key::Delete,
                        "enter" => Key::Enter,
                        "left" => Key::ArrowLeft,
                        "right" => Key::ArrowRight,
                        "ime-start" => {
                            input.events.push(Event::Ime(ImeEvent::Enabled));
                            continue;
                        }
                        _ => {
                            if let Some(text) = item.strip_prefix("text:") {
                                if !text.is_empty() {
                                    input.events.push(Event::Text(text.into()));
                                }
                            } else if let Some(text) = item.strip_prefix("preedit:") {
                                input
                                    .events
                                    .push(Event::Ime(ImeEvent::Preedit(text.into())));
                            } else if let Some(text) = item.strip_prefix("commit:") {
                                input.events.push(Event::Ime(ImeEvent::Commit(text.into())));
                            }
                            continue;
                        }
                    };
                    input.events.push(Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    });
                }
            });
        }
    }

    pub(crate) fn update_ime(&mut self, ctx: &Context) {
        let ime = ctx.output(|o| o.ime);
        let next = ime.and_then(|_| ctx.memory(|m| m.focused()));
        let focus_changed = next != self.ime_focus;
        let tapped_field = ime.is_some_and(|ime| {
            ctx.input(|input| {
                input.pointer.any_released()
                    && input
                        .pointer
                        .interact_pos()
                        .is_some_and(|pos| ime.rect.contains(pos))
            })
        });
        if !focus_changed && !tapped_field {
            return;
        }
        self.ime_focus = next;
        let result = self.call(|env, activity| {
            env.call_method(
                activity,
                jni_str!("setKeyboard"),
                jni_sig!((bool, bool) -> ()),
                &[JValue::Bool(next.is_some()), JValue::Bool(focus_changed)],
            )?;
            Ok(())
        });
        if let Err(error) = result {
            eprintln!("xmath: {error}");
        }
    }
}
