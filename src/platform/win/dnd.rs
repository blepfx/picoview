use crate::Exchange;
use crate::platform::win::util::decode_hdrop;
use crate::platform::win::window::{
    WM_USER_DND_ACCEPT, WM_USER_DND_ENTER, WM_USER_DND_HOVER, WM_USER_DND_LEAVE,
};
use com::{IDataObject, IDropTarget, IDropTargetVtbl, IUnknown, IUnknownVtbl};
use std::ffi::c_void;
use std::mem::zeroed;
use std::ptr::null_mut;
use std::sync::Arc;
use windows_sys::Win32::Foundation::{E_NOINTERFACE, HWND, POINT, S_OK};
use windows_sys::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, STGMEDIUM, TYMED_HGLOBAL};
use windows_sys::Win32::System::Ole::{
    CF_HDROP, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, SendMessageW};
use windows_sys::core::{GUID, HRESULT};

#[repr(C)]
pub struct DropTargetImpl {
    base: IDropTarget,
    hwnd: HWND,
}

unsafe impl Send for DropTargetImpl {}
unsafe impl Sync for DropTargetImpl {}

impl DropTargetImpl {
    const VTABLE: IDropTargetVtbl = IDropTargetVtbl {
        unknown: IUnknownVtbl {
            query_interface: Self::query_interface,
            add_ref: Self::add_ref,
            release: Self::release,
        },
        drag_enter: Self::drag_enter,
        drag_over: Self::drag_over,
        drag_leave: Self::drag_leave,
        drag_drop: Self::drag_drop,
    };

    pub fn new(hwnd: HWND) -> Arc<Self> {
        Arc::new(Self {
            base: IDropTarget {
                vtbl: &Self::VTABLE,
            },
            hwnd,
        })
    }

    pub fn as_raw(data: &Arc<Self>) -> *mut IDropTarget {
        Arc::as_ptr(data) as *mut IDropTarget
    }

    unsafe extern "system" fn query_interface(
        _: *mut IUnknown,
        _: *const GUID,
        _: *mut *mut c_void,
    ) -> HRESULT {
        E_NOINTERFACE
    }

    unsafe extern "system" fn add_ref(this: *mut IUnknown) -> u32 {
        unsafe {
            let this = this as *const Self;
            Arc::increment_strong_count(this);
            let this = Arc::from_raw(this);
            let count = Arc::strong_count(&this);
            let _ = Arc::into_raw(this); // prevent dropping
            count as u32
        }
    }

    unsafe extern "system" fn release(this: *mut IUnknown) -> u32 {
        unsafe {
            let this = this as *const Self;
            let this = Arc::from_raw(this);
            let count = Arc::strong_count(&this) - 1;
            drop(this); // drop the Arc, which may deallocate if count reaches 0
            count as u32
        }
    }

    unsafe extern "system" fn drag_enter(
        this: *mut IDropTarget,
        data: *const IDataObject,
        _: u32,
        point: POINT,
        pdw_effect: *mut u32,
    ) -> HRESULT {
        unsafe {
            let this = &*(this as *const Self);

            // we use SendMessage here because the data object _might_ expire and the point
            // is only valid during the call
            SendMessageW(
                this.hwnd,
                WM_USER_DND_ENTER,
                data as usize,
                &point as *const POINT as isize,
            );

            pdw_effect.write(DROPEFFECT_LINK | DROPEFFECT_COPY | DROPEFFECT_MOVE);
            S_OK
        }
    }

    unsafe extern "system" fn drag_over(
        this: *mut IDropTarget,
        _: u32,
        point: POINT,
        pdw_effect: *mut u32,
    ) -> HRESULT {
        unsafe {
            let this = &*(this as *const Self);

            // same goes for this as in drag_enter
            SendMessageW(
                this.hwnd,
                WM_USER_DND_HOVER,
                0,
                &point as *const POINT as isize,
            );

            pdw_effect.write(DROPEFFECT_LINK | DROPEFFECT_COPY | DROPEFFECT_MOVE);
            S_OK
        }
    }

    unsafe extern "system" fn drag_leave(this: *mut IDropTarget) -> HRESULT {
        unsafe {
            let this = &*(this as *const Self);
            PostMessageW(this.hwnd, WM_USER_DND_LEAVE, 0, 0);
            S_OK
        }
    }

    unsafe extern "system" fn drag_drop(
        this: *mut IDropTarget,
        _: *const IDataObject,
        _: u32,
        _: POINT,
        pdw_effect: *mut u32,
    ) -> HRESULT {
        unsafe {
            let this = &*(this as *const Self);
            PostMessageW(this.hwnd, WM_USER_DND_ACCEPT, 0, 0);
            pdw_effect.write(DROPEFFECT_LINK | DROPEFFECT_COPY | DROPEFFECT_MOVE);
            S_OK
        }
    }

    pub unsafe fn decode_data_object(data: *mut IDataObject) -> Exchange {
        unsafe {
            let mut medium = STGMEDIUM { ..zeroed() };
            let format = FORMATETC {
                cfFormat: CF_HDROP,
                dwAspect: DVASPECT_CONTENT,
                tymed: TYMED_HGLOBAL as _,
                ptd: null_mut(),
                lindex: -1,
            };

            if ((*(*data).vtbl).get_data)(data, &format, &mut medium) == S_OK {
                let files = decode_hdrop(medium.u.hGlobal);
                return Exchange::Files(files);
            }

            Exchange::Empty
        }
    }
}

mod com {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::System::Com::{FORMATETC, STGMEDIUM};
    use windows_sys::core::{GUID, HRESULT};

    pub type IUnknown = *mut c_void;

    #[repr(C)]
    pub struct IUnknownVtbl {
        pub query_interface: unsafe extern "system" fn(
            this: *mut IUnknown,
            riid: *const GUID,
            ppv_object: *mut *mut c_void,
        ) -> HRESULT,
        pub add_ref: unsafe extern "system" fn(this: *mut IUnknown) -> u32,
        pub release: unsafe extern "system" fn(this: *mut IUnknown) -> u32,
    }

    #[repr(C)]
    pub struct IDropTargetVtbl {
        pub unknown: IUnknownVtbl,
        pub drag_enter: unsafe extern "system" fn(
            this: *mut IDropTarget,
            data_obj: *const IDataObject,
            key_state: u32,
            pt: POINT,
            pdw_effect: *mut u32,
        ) -> HRESULT,
        pub drag_over: unsafe extern "system" fn(
            this: *mut IDropTarget,
            key_state: u32,
            pt: POINT,
            pdw_effect: *mut u32,
        ) -> HRESULT,
        pub drag_leave: unsafe extern "system" fn(this: *mut IDropTarget) -> HRESULT,
        pub drag_drop: unsafe extern "system" fn(
            this: *mut IDropTarget,
            data_obj: *const IDataObject,
            key_state: u32,
            pt: POINT,
            pdw_effect: *mut u32,
        ) -> HRESULT,
    }

    #[repr(C)]
    pub struct IDataObjectVtbl {
        pub parent: IUnknownVtbl,
        pub get_data: unsafe extern "system" fn(
            this: *mut IDataObject,
            pformatetc_in: *const FORMATETC,
            pmedium: *mut STGMEDIUM,
        ) -> HRESULT,

        // there are other methods but we dont need them
        _private: (),
    }

    #[repr(C)]
    pub struct IDropTarget {
        pub vtbl: *const IDropTargetVtbl,
    }

    #[repr(C)]
    pub struct IDataObject {
        pub vtbl: *const IDataObjectVtbl,
    }
}
