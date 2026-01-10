#![allow(deprecated)] // i love you apple <3

use crate::{Error, GlConfig, GlVersion};
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, rc::Retained};
use objc2_app_kit::{NSOpenGLContext, NSOpenGLPixelFormat, NSOpenGLView, NSView};
use objc2_core_foundation::{CFBundle, CFRetained, CFString};
use objc2_foundation::NSSize;
use std::{fmt::Debug, ptr::NonNull};

pub struct GlContext {
    bundle: CFRetained<CFBundle>,
    context: Retained<NSOpenGLContext>,
    view: Retained<NSOpenGLView>,
}

impl GlContext {
    pub fn new(parent: &NSView, config: GlConfig, mtm: MainThreadMarker) -> Result<Self, Error> {
        let version = match config.version {
            GlVersion::Core(major, minor) => {
                if (major, minor) > (4, 1) {
                    return Err(Error::OpenGlError(
                        "macOS only supports OpenGL up to version 4.1".into(),
                    ));
                } else if (major, minor) > (3, 2) {
                    objc2_app_kit::NSOpenGLProfileVersion4_1Core
                } else {
                    objc2_app_kit::NSOpenGLProfileVersion3_2Core
                }
            }
            GlVersion::Compat(_, _) => objc2_app_kit::NSOpenGLProfileVersionLegacy,
            GlVersion::ES(_, _) => {
                return Err(Error::OpenGlError(
                    "macOS does not support OpenGL ES contexts".into(),
                ));
            }
        };

        let attrs = {
            let (r, g, b, a, d, s) = config.format.as_rgbads();
            let mut attrs = vec![
                objc2_app_kit::NSOpenGLPFAOpenGLProfile,
                version,
                objc2_app_kit::NSOpenGLPFAColorSize,
                (r + g + b) as _,
                objc2_app_kit::NSOpenGLPFAAlphaSize,
                a as _,
                objc2_app_kit::NSOpenGLPFADepthSize,
                d as _,
                objc2_app_kit::NSOpenGLPFAStencilSize,
                s as _,
            ];

            if config.optional {
                attrs.push(objc2_app_kit::NSOpenGLPFAAccelerated); // TODO: allow software rendering?
            }

            if config.double_buffer {
                attrs.push(objc2_app_kit::NSOpenGLPFADoubleBuffer);
            }

            if config.msaa_count > 0 {
                attrs.extend_from_slice(&[
                    objc2_app_kit::NSOpenGLPFAMultisample,
                    objc2_app_kit::NSOpenGLPFASampleBuffers,
                    1,
                    objc2_app_kit::NSOpenGLPFASamples,
                    config.msaa_count as _,
                ]);
            }

            attrs.push(0);
            attrs
        };

        let pixel_format = unsafe {
            NSOpenGLPixelFormat::initWithAttributes(
                NSOpenGLPixelFormat::alloc(),
                NonNull::new_unchecked(attrs.as_ptr() as *mut _),
            )
            .ok_or_else(|| Error::OpenGlError("Failed to create NSOpenGLPixelFormat".into()))?
        };

        let view = {
            NSOpenGLView::initWithFrame_pixelFormat(
                NSOpenGLView::alloc(mtm),
                parent.frame(),
                Some(&pixel_format),
            )
            .ok_or_else(|| Error::OpenGlError("Failed to create NSOpenGLView".into()))?
        };

        view.setWantsBestResolutionOpenGLSurface(true);
        view.setWantsLayer(true);
        view.display();
        parent.addSubview(&view);

        let context = view.openGLContext().ok_or_else(|| {
            Error::OpenGlError("Failed to get NSOpenGLContext from NSOpenGLView".into())
        })?;

        unsafe {
            context
                .setValues_forParameter(NonNull::from(&0), objc2_app_kit::NSOpenGLCPSwapInterval);
        }

        let bundle = {
            CFBundle::bundle_with_identifier(Some(&CFString::from_static_str("com.apple.opengl")))
                .ok_or_else(|| Error::OpenGlError("Failed to get main CFBundle for OpenGL".into()))?
        };

        Ok(Self {
            context,
            view,
            bundle,
        })
    }

    pub fn resize(&self, width: u32, height: u32) {
        self.view.setFrameSize(NSSize {
            width: width as f64,
            height: height as f64,
        });

        self.view.setNeedsDisplay(true);
    }
}

impl crate::GlContext for GlContext {
    fn make_current(&self, current: bool) -> bool {
        if current {
            self.context.makeCurrentContext();
        } else {
            NSOpenGLContext::clearCurrentContext();
        }

        true
    }

    fn swap_buffers(&self) {
        self.context.flushBuffer();
        self.view.setNeedsDisplay(true); // TODO: do we need this?
    }

    fn get_proc_address(&self, name: &std::ffi::CStr) -> *const std::ffi::c_void {
        match name.to_str() {
            Err(_) => std::ptr::null(),
            Ok(name) => {
                CFBundle::function_pointer_for_name(&self.bundle, Some(&CFString::from_str(name)))
            }
        }
    }
}

impl Debug for GlContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlContext").finish_non_exhaustive()
    }
}
