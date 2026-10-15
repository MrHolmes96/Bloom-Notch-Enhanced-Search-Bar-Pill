//! Native jump lists for the dock's right-click menu.
//!
//! Windows has no public API for reading another app's jump list. Explorer's
//! own taskbar reads it through three undocumented shell objects, and so does
//! this module (the same objects Open-Shell has relied on since Windows 7):
//!
//! - the app resolver, which gives a window or shortcut its AppUserModelID,
//! - the automatic destination list (pinned, recent and frequent items),
//! - the custom destination list (an app's own categories and tasks).
//!
//! Their interfaces are not in the SDK. If a Windows update changes them, the
//! queries fail and the menu has no jump list; nothing else is affected.

// The COM method names mirror the Windows interfaces they bind to.
#![allow(non_snake_case)]

use serde::Serialize;
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::{LazyLock, Mutex};
use windows::core::{IUnknown, IUnknown_Vtbl, Interface, BOOL, GUID, HRESULT, PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, PROPERTYKEY};
use windows::Win32::System::Com::StructuredStorage::{
    PropVariantClear, PropVariantToBoolean, PropVariantToStringAlloc,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistStream,
    CLSCTX_INPROC_HANDLER, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, STREAM_SEEK_CUR,
    STREAM_SEEK_SET,
};
use windows::Win32::UI::Shell::Common::IObjectArray;
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::Shell::{
    IShellItem, IShellLinkW, SHCreateItemFromParsingName, SHCreateMemStream, SHGetNameFromIDList,
    SHLoadIndirectString, ShellLink, SIGDN, SIGDN_DESKTOPABSOLUTEPARSING, SIGDN_NORMALDISPLAY,
};

/// Items shown per group, matching the taskbar's default.
const MAX_ITEMS: u32 = 10;

const CLSID_APP_RESOLVER: GUID = GUID::from_u128(0x660b90c8_73a9_4b58_8cae_355b7f55341b);
const CLSID_AUTOMATIC_DESTINATION_LIST: GUID =
    GUID::from_u128(0xf0ae1542_f497_484b_a175_a20db09144ba);
const CLSID_DESTINATION_LIST: GUID = GUID::from_u128(0x77f10cf0_3db5_4966_b520_b7c54fd35ed6);
/// The custom destination list interface as shipped in Windows 10 build 10240.
/// Later builds use the IID on [`IDestinationList`]; the layout is the same.
const IID_DESTINATION_LIST_10240: GUID = GUID::from_u128(0xfebd543d_1f7b_4b38_940b_5933bd2cb21b);

const fn appmodel_key(pid: u32) -> PROPERTYKEY {
    PROPERTYKEY {
        fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
        pid,
    }
}
const PKEY_TITLE: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xf29f85e0_4ff9_1068_ab91_08002b27b3d9),
    pid: 2,
};
const PKEY_APP_USER_MODEL_ID: PROPERTYKEY = appmodel_key(5);
const PKEY_IS_DEST_LIST_SEPARATOR: PROPERTYKEY = appmodel_key(6);
const PKEY_HOST_ENVIRONMENT: PROPERTYKEY = appmodel_key(14);
const PKEY_ACTIVATION_CONTEXT: PROPERTYKEY = appmodel_key(20);
const PKEY_LINK_ARGUMENTS: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x436f2667_14e2_4feb_b30a_146c53b5b674),
    pid: 100,
};

#[windows::core::interface("de25675a-72de-44b4-9373-05170450c140")]
unsafe trait IAppResolver8: IUnknown {
    fn GetAppIDForShortcut(&self, item: *mut c_void, app_id: *mut PWSTR) -> HRESULT;
    fn GetAppIDForShortcutObject(&self) -> HRESULT;
    fn GetAppIDForWindow(
        &self,
        hwnd: HWND,
        app_id: *mut PWSTR,
        reserved1: *mut c_void,
        reserved2: *mut c_void,
        reserved3: *mut c_void,
    ) -> HRESULT;
}

#[repr(C)]
struct AppDestCategory {
    /// 0 = the app's own named category, 1 = a known category, 2 = tasks.
    kind: i32,
    /// The category name for kind 0; for kind 1 its low 32 bits hold the
    /// known category (1 = frequent, 2 = recent).
    name_or_known: *mut u16,
    count: i32,
    reserved: [i32; 10],
}

#[windows::core::interface("507101cd-f6ad-46c8-8e20-eeb9e6bac47f")]
unsafe trait IDestinationList: IUnknown {
    fn SetMinItems(&self) -> HRESULT;
    fn SetApplicationID(&self, app_id: PCWSTR) -> HRESULT;
    fn GetSlotCount(&self) -> HRESULT;
    fn GetCategoryCount(&self, count: *mut u32) -> HRESULT;
    fn GetCategory(&self, index: u32, flags: i32, category: *mut AppDestCategory) -> HRESULT;
    fn DeleteCategory(&self) -> HRESULT;
    fn EnumerateCategoryDestinations(
        &self,
        index: u32,
        riid: *const GUID,
        object: *mut *mut c_void,
    ) -> HRESULT;
}

#[windows::core::interface("e9c5ef8d-fd41-4f72-ba87-eb03bad5817c")]
unsafe trait IAutomaticDestinationList: IUnknown {
    fn Initialize(&self, app_id: PCWSTR, shortcut: PCWSTR, reserved: PCWSTR) -> HRESULT;
    fn HasList(&self, has_list: *mut BOOL) -> HRESULT;
    fn GetList(
        &self,
        list: i32,
        max_count: u32,
        flags: u32,
        riid: *const GUID,
        object: *mut *mut c_void,
    ) -> HRESULT;
}

/// Lists kept by the automatic destination list.
const LIST_PINNED: i32 = 0;
const LIST_RECENT: i32 = 1;
const LIST_FREQUENT: i32 = 2;

#[derive(Serialize, Clone)]
pub struct JumpItem {
    /// Handle for `open_jump_item`; valid until the next `get_jump_list`.
    pub id: u64,
    pub title: String,
    pub separator: bool,
}

#[derive(Serialize, Clone)]
pub struct JumpGroup {
    pub name: String,
    /// "pinned", "recent", "frequent", "custom" or "tasks".
    pub kind: &'static str,
    pub items: Vec<JumpItem>,
}

/// What a jump list entry opens. Shell objects cannot leave the thread that
/// read them, so shortcuts are kept as their serialized bytes and reloaded on
/// click.
enum Target {
    Item {
        parsing_name: String,
        app_exe: Option<String>,
    },
    Link(Vec<u8>),
}

static TARGETS: LazyLock<Mutex<HashMap<u64, Target>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Takes ownership of a CoTaskMem string.
unsafe fn take_pwstr(p: PWSTR) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let s = p.to_string().ok();
    CoTaskMemFree(Some(p.0 as *const c_void));
    s.filter(|s| !s.is_empty())
}

/// Resolves "@shell32.dll,-1234" style resource strings.
unsafe fn load_indirect(s: &str) -> String {
    if !s.starts_with('@') {
        return s.to_string();
    }
    let source = wide(s);
    let mut buf = [0u16; 512];
    if SHLoadIndirectString(PCWSTR(source.as_ptr()), &mut buf, None).is_ok() {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        return String::from_utf16_lossy(&buf[..len]);
    }
    s.to_string()
}

unsafe fn store_string(store: &IPropertyStore, key: &PROPERTYKEY) -> Option<String> {
    let mut value = store.GetValue(key).ok()?;
    let result = PropVariantToStringAlloc(&value)
        .ok()
        .and_then(|p| take_pwstr(p));
    let _ = PropVariantClear(&mut value);
    result
}

unsafe fn store_bool(store: &IPropertyStore, key: &PROPERTYKEY) -> bool {
    let Ok(mut value) = store.GetValue(key) else {
        return false;
    };
    let result = PropVariantToBoolean(&value).is_ok_and(|b| b.as_bool());
    let _ = PropVariantClear(&mut value);
    result
}

unsafe fn item_name(item: &IShellItem, sigdn: SIGDN) -> Option<String> {
    item.GetDisplayName(sigdn).ok().and_then(|p| take_pwstr(p))
}

/// Finds the AppUserModelID Explorer files the app's jump list under.
unsafe fn resolve_app_id(path: &str, hwnd: Option<isize>) -> Option<String> {
    let resolver: IAppResolver8 = CoCreateInstance(
        &CLSID_APP_RESOLVER,
        None,
        CLSCTX_INPROC_SERVER | CLSCTX_INPROC_HANDLER,
    )
    .ok()?;

    if let Some(hwnd) = hwnd.filter(|h| *h != 0) {
        let mut id = PWSTR::null();
        let hr = resolver.GetAppIDForWindow(
            HWND(hwnd as *mut c_void),
            &mut id,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        if hr.is_ok() {
            if let Some(id) = take_pwstr(id) {
                return Some(id);
            }
        }
    }

    // Packaged apps and PWAs are pinned by their AppUserModelID, which is
    // already the id the jump list is filed under.
    if crate::commands::is_aumid_path(path) {
        let id = path.trim().trim_matches('"');
        let id = if id.to_lowercase().starts_with("shell:appsfolder\\") {
            &id["shell:appsfolder\\".len()..]
        } else {
            id
        };
        return Some(id.to_string());
    }

    let wide_path = wide(path);
    let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide_path.as_ptr()), None).ok()?;
    let mut id = PWSTR::null();
    if resolver.GetAppIDForShortcut(item.as_raw(), &mut id).is_ok() {
        return take_pwstr(id);
    }
    None
}

/// The executable recent files should open in, like the taskbar does.
fn app_executable(path: &str, hwnd: Option<isize>) -> Option<String> {
    let exe = if path.to_lowercase().ends_with(".exe") {
        path.to_string()
    } else {
        let hwnd = hwnd.filter(|h| *h != 0)?;
        let mut pid = 0u32;
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
                HWND(hwnd as *mut c_void),
                Some(&mut pid),
            );
            if pid == 0 {
                return None;
            }
            crate::commands::process_image_path(pid)?
        }
    };
    // Packaged apps run from WindowsApps and can't be launched by path, and a
    // browser or Explorer host can't tell which of its apps a file is for;
    // those open through the file's default handler instead.
    let lower = exe.to_lowercase();
    if lower.contains("\\windowsapps\\") || lower.ends_with("\\applicationframehost.exe") {
        return None;
    }
    Some(exe)
}

unsafe fn custom_list(app_id: &str) -> Option<IDestinationList> {
    let unknown: IUnknown =
        CoCreateInstance(&CLSID_DESTINATION_LIST, None, CLSCTX_INPROC_SERVER).ok()?;
    let list = match unknown.cast::<IDestinationList>() {
        Ok(list) => list,
        Err(_) => {
            let mut raw = std::ptr::null_mut();
            unknown
                .query(&IID_DESTINATION_LIST_10240, &mut raw)
                .ok()
                .ok()?;
            IDestinationList::from_raw(raw)
        }
    };
    let id = wide(app_id);
    list.SetApplicationID(PCWSTR(id.as_ptr())).ok().ok()?;
    Some(list)
}

unsafe fn automatic_list(app_id: &str) -> Option<IAutomaticDestinationList> {
    let list: IAutomaticDestinationList = CoCreateInstance(
        &CLSID_AUTOMATIC_DESTINATION_LIST,
        None,
        CLSCTX_INPROC_SERVER,
    )
    .ok()?;
    let id = wide(app_id);
    list.Initialize(PCWSTR(id.as_ptr()), PCWSTR::null(), PCWSTR::null())
        .ok()
        .ok()?;
    Some(list)
}

unsafe fn automatic_items(list: &IAutomaticDestinationList, which: i32) -> Option<IObjectArray> {
    let mut raw = std::ptr::null_mut();
    list.GetList(which, MAX_ITEMS, 1, &IObjectArray::IID, &mut raw)
        .ok()
        .ok()?;
    if raw.is_null() {
        return None;
    }
    Some(IObjectArray::from_raw(raw))
}

unsafe fn save_link(link: &IShellLinkW) -> Option<Vec<u8>> {
    let persist: IPersistStream = link.cast().ok()?;
    let stream = SHCreateMemStream(None)?;
    persist.Save(&stream, false).ok()?;
    let mut size = 0u64;
    stream.Seek(0, STREAM_SEEK_CUR, Some(&mut size)).ok()?;
    stream.Seek(0, STREAM_SEEK_SET, None).ok()?;
    let mut bytes = vec![0u8; size as usize];
    let mut read = 0u32;
    stream
        .Read(
            bytes.as_mut_ptr() as *mut c_void,
            size as u32,
            Some(&mut read),
        )
        .ok()
        .ok()?;
    bytes.truncate(read as usize);
    Some(bytes)
}

unsafe fn load_link(bytes: &[u8]) -> Option<IShellLinkW> {
    let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
    let persist: IPersistStream = link.cast().ok()?;
    let stream = SHCreateMemStream(Some(bytes))?;
    persist.Load(&stream).ok()?;
    Some(link)
}

struct Builder {
    next_id: u64,
    targets: HashMap<u64, Target>,
    /// Items already shown, so recent/frequent don't repeat pinned ones.
    seen: Vec<String>,
    app_exe: Option<String>,
}

impl Builder {
    unsafe fn collect(
        &mut self,
        array: &IObjectArray,
        is_tasks: bool,
        dedupe: bool,
    ) -> Vec<JumpItem> {
        let mut items = Vec::new();
        let count = array.GetCount().unwrap_or(0);
        for i in 0..count {
            let Ok(object) = array.GetAt::<IUnknown>(i) else {
                continue;
            };
            if let Some(item) = self.add(&object, is_tasks, dedupe) {
                items.push(item);
            }
            if !is_tasks && items.len() >= MAX_ITEMS as usize {
                break;
            }
        }
        // Drop separators at the edges and doubled ones.
        let mut cleaned: Vec<JumpItem> = Vec::with_capacity(items.len());
        for item in items {
            if item.separator && cleaned.last().is_none_or(|last| last.separator) {
                continue;
            }
            cleaned.push(item);
        }
        while cleaned.last().is_some_and(|last| last.separator) {
            cleaned.pop();
        }
        cleaned
    }

    unsafe fn add(&mut self, object: &IUnknown, is_tasks: bool, dedupe: bool) -> Option<JumpItem> {
        if let Ok(item) = object.cast::<IShellItem>() {
            let title = item_name(&item, SIGDN_NORMALDISPLAY)?;
            let parsing_name = item_name(&item, SIGDN_DESKTOPABSOLUTEPARSING)?;
            if !self.mark_seen(parsing_name.to_uppercase(), dedupe) {
                return None;
            }
            let app_exe = self.app_exe.clone();
            return Some(self.push(
                title,
                false,
                Target::Item {
                    parsing_name,
                    app_exe,
                },
            ));
        }

        let link = object.cast::<IShellLinkW>().ok()?;
        let store = link.cast::<IPropertyStore>().ok();
        if is_tasks
            && store
                .as_ref()
                .is_some_and(|s| store_bool(s, &PKEY_IS_DEST_LIST_SEPARATOR))
        {
            return Some(JumpItem {
                id: 0,
                title: String::new(),
                separator: true,
            });
        }

        let mut title = store
            .as_ref()
            .and_then(|s| store_string(s, &PKEY_TITLE))
            .map(|t| load_indirect(&t));
        let mut key = String::new();
        if let Ok(pidl) = link.GetIDList() {
            if !pidl.is_null() {
                if title.is_none() {
                    title = SHGetNameFromIDList(pidl, SIGDN_NORMALDISPLAY)
                        .ok()
                        .and_then(|p| take_pwstr(p));
                }
                key = SHGetNameFromIDList(pidl, SIGDN_DESKTOPABSOLUTEPARSING)
                    .ok()
                    .and_then(|p| take_pwstr(p))
                    .unwrap_or_default()
                    .to_uppercase();
                CoTaskMemFree(Some(pidl as *const c_void));
            }
        }
        let title = title.filter(|t| !t.trim().is_empty())?;
        if let Some(args) = store
            .as_ref()
            .and_then(|s| store_string(s, &PKEY_LINK_ARGUMENTS))
        {
            key.push('|');
            key.push_str(&args);
        }
        if !self.mark_seen(key, dedupe) {
            return None;
        }
        let bytes = save_link(&link)?;
        Some(self.push(title, false, Target::Link(bytes)))
    }

    /// Returns false when the key was already shown and should be skipped.
    fn mark_seen(&mut self, key: String, dedupe: bool) -> bool {
        if key.is_empty() {
            return true;
        }
        if dedupe && self.seen.contains(&key) {
            return false;
        }
        self.seen.push(key);
        true
    }

    fn push(&mut self, title: String, separator: bool, target: Target) -> JumpItem {
        self.next_id += 1;
        self.targets.insert(self.next_id, target);
        JumpItem {
            id: self.next_id,
            title,
            separator,
        }
    }
}

unsafe fn read_jump_list(app_id: &str, app_exe: Option<String>) -> Vec<JumpGroup> {
    let mut builder = Builder {
        next_id: 0,
        targets: HashMap::new(),
        seen: Vec::new(),
        app_exe,
    };
    let mut groups: Vec<JumpGroup> = Vec::new();
    let auto = automatic_list(app_id);

    if let Some(pinned) = auto.as_ref().and_then(|a| automatic_items(a, LIST_PINNED)) {
        let items = builder.collect(&pinned, false, false);
        groups.push(JumpGroup {
            name: "Pinned".into(),
            kind: "pinned",
            items,
        });
    }

    let custom = custom_list(app_id);
    let mut category_count = 0u32;
    if let Some(list) = custom.as_ref() {
        if list.GetCategoryCount(&mut category_count).is_err() {
            category_count = 0;
        }
    }

    let mut tasks_index: Option<u32> = None;
    if let Some(list) = custom.as_ref() {
        for index in 0..category_count {
            let mut category: AppDestCategory = std::mem::zeroed();
            if list.GetCategory(index, 1, &mut category).is_err() {
                continue;
            }
            match category.kind {
                0 => {
                    if category.name_or_known.is_null() {
                        continue;
                    }
                    let raw = PWSTR(category.name_or_known);
                    let name = take_pwstr(raw)
                        .map(|n| load_indirect(&n))
                        .unwrap_or_default();
                    let mut object = std::ptr::null_mut();
                    if list
                        .EnumerateCategoryDestinations(index, &IObjectArray::IID, &mut object)
                        .is_ok()
                        && !object.is_null()
                    {
                        let array = IObjectArray::from_raw(object);
                        let items = builder.collect(&array, false, true);
                        groups.push(JumpGroup {
                            name,
                            kind: "custom",
                            items,
                        });
                    }
                }
                1 => {
                    let known = category.name_or_known as usize as u32;
                    let (which, name, kind) = match known {
                        1 => (LIST_FREQUENT, "Frequent", "frequent"),
                        2 => (LIST_RECENT, "Recent", "recent"),
                        _ => continue,
                    };
                    if let Some(array) = auto.as_ref().and_then(|a| automatic_items(a, which)) {
                        let items = builder.collect(&array, false, true);
                        groups.push(JumpGroup {
                            name: name.into(),
                            kind,
                            items,
                        });
                    }
                }
                2 if tasks_index.is_none() => tasks_index = Some(index),
                _ => {}
            }
        }
    }

    // Apps that never customized their jump list get the system's recent list.
    if category_count == 0 {
        if let Some(array) = auto.as_ref().and_then(|a| automatic_items(a, LIST_RECENT)) {
            let items = builder.collect(&array, false, true);
            groups.push(JumpGroup {
                name: "Recent".into(),
                kind: "recent",
                items,
            });
        }
    }

    if let (Some(list), Some(index)) = (custom.as_ref(), tasks_index) {
        let mut object = std::ptr::null_mut();
        if list
            .EnumerateCategoryDestinations(index, &IObjectArray::IID, &mut object)
            .is_ok()
            && !object.is_null()
        {
            let array = IObjectArray::from_raw(object);
            let items = builder.collect(&array, true, false);
            groups.push(JumpGroup {
                name: "Tasks".into(),
                kind: "tasks",
                items,
            });
        }
    }

    groups.retain(|g| g.items.iter().any(|i| !i.separator));
    if let Ok(mut targets) = TARGETS.lock() {
        *targets = builder.targets;
    }
    groups
}

/// The jump list for a dock item: the app's pinned, recent/frequent and own
/// categories, then its tasks. Empty when the app has none or Windows doesn't
/// expose it.
#[tauri::command]
pub async fn get_jump_list(path: String, hwnd: Option<isize>) -> Vec<JumpGroup> {
    tauri::async_runtime::spawn_blocking(move || unsafe {
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let groups = match resolve_app_id(&path, hwnd) {
            Some(app_id) => read_jump_list(&app_id, app_executable(&path, hwnd)),
            None => Vec::new(),
        };
        if com {
            CoUninitialize();
        }
        groups
    })
    .await
    .unwrap_or_default()
}

/// Runs a shortcut the way the taskbar does: through its default verb, or for
/// packaged apps through the app's own activation with its saved context.
unsafe fn invoke_link(link: &IShellLinkW) -> bool {
    use windows::Win32::UI::Shell::{
        BHID_SFUIObject, FOLDERID_AppsFolder, IContextMenu, IShellItem2, SHCreateItemInKnownFolder,
        CMF_DEFAULTONLY, CMINVOKECOMMANDINFO, KF_FLAG_DEFAULT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreatePopupMenu, DestroyMenu, GetMenuDefaultItem, GET_MENU_DEFAULT_ITEM_FLAGS,
        SW_SHOWNORMAL,
    };
    const CMIC_MASK_FLAG_LOG_USAGE: u32 = 0x0400_0000;

    let mut menu_source: Option<IContextMenu> = link.cast().ok();
    let mut params: Option<std::ffi::CString> = None;

    if let Ok(store) = link.cast::<IPropertyStore>() {
        if let Some(app_id) = store_string(&store, &PKEY_APP_USER_MODEL_ID) {
            let id = wide(&app_id);
            if let Ok(target) = SHCreateItemInKnownFolder::<_, IShellItem2>(
                &FOLDERID_AppsFolder,
                KF_FLAG_DEFAULT,
                PCWSTR(id.as_ptr()),
            ) {
                if target.GetUInt32(&PKEY_HOST_ENVIRONMENT).unwrap_or(0) != 0 {
                    if let Ok(menu) =
                        target.BindToHandler::<_, IContextMenu>(None, &BHID_SFUIObject)
                    {
                        menu_source = Some(menu);
                        params = store_string(&store, &PKEY_ACTIVATION_CONTEXT)
                            .and_then(|c| std::ffi::CString::new(c).ok());
                    }
                }
            }
        }
    }

    let Some(context_menu) = menu_source else {
        return false;
    };
    let Ok(menu) = CreatePopupMenu() else {
        return false;
    };
    let mut ok = false;
    if context_menu
        .QueryContextMenu(menu, 0, 1, 1000, CMF_DEFAULTONLY)
        .is_ok()
    {
        let id = GetMenuDefaultItem(menu, 0, GET_MENU_DEFAULT_ITEM_FLAGS(0));
        if id != u32::MAX && id > 0 {
            let info = CMINVOKECOMMANDINFO {
                cbSize: std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32,
                fMask: CMIC_MASK_FLAG_LOG_USAGE,
                lpVerb: windows::core::PCSTR((id - 1) as usize as *const u8),
                lpParameters: params.as_ref().map_or(windows::core::PCSTR::null(), |p| {
                    windows::core::PCSTR(p.as_ptr() as *const u8)
                }),
                nShow: SW_SHOWNORMAL.0,
                ..Default::default()
            };
            ok = context_menu.InvokeCommand(&info).is_ok();
        }
    }
    let _ = DestroyMenu(menu);
    ok
}

/// Opens a recent or pinned file in the app it belongs to, falling back to the
/// file's default handler.
unsafe fn open_item(parsing_name: &str, app_exe: Option<&str>) {
    use windows::Win32::UI::Shell::{
        SHGetIDListFromObject, ShellExecuteExW, ShellExecuteW, SEE_MASK_FLAG_LOG_USAGE,
        SEE_MASK_IDLIST, SHELLEXECUTEINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    if let Some(exe) = app_exe {
        let file = wide(exe);
        let args = wide(&format!("\"{}\"", parsing_name));
        let verb = wide("open");
        let result = ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR(args.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
        if result.0 as usize > 32 {
            return;
        }
    }

    let name = wide(parsing_name);
    let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(name.as_ptr()), None)
    else {
        return;
    };
    let Ok(pidl) = SHGetIDListFromObject(&item) else {
        return;
    };
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_IDLIST | SEE_MASK_FLAG_LOG_USAGE,
        lpIDList: pidl as *mut c_void,
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    let _ = ShellExecuteExW(&mut info);
    CoTaskMemFree(Some(pidl as *const c_void));
}

#[tauri::command]
pub async fn open_jump_item(id: u64) {
    let target = TARGETS.lock().ok().and_then(|mut t| t.remove(&id));
    let Some(target) = target else {
        return;
    };
    let _ = tauri::async_runtime::spawn_blocking(move || unsafe {
        let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        match &target {
            Target::Item {
                parsing_name,
                app_exe,
            } => open_item(parsing_name, app_exe.as_deref()),
            Target::Link(bytes) => {
                if let Some(link) = load_link(bytes) {
                    if !invoke_link(&link) {
                        fallback_run_link(&link);
                    }
                }
            }
        }
        if com {
            CoUninitialize();
        }
    })
    .await;
}

/// Last resort for a shortcut whose context menu can't be invoked: run its
/// target with its arguments.
unsafe fn fallback_run_link(link: &IShellLinkW) {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let mut path = [0u16; 1024];
    if link.GetPath(&mut path, std::ptr::null_mut(), 0).is_err() || path[0] == 0 {
        return;
    }
    let mut args = [0u16; 2048];
    let _ = link.GetArguments(&mut args);
    let verb = wide("open");
    let _ = ShellExecuteW(
        None,
        PCWSTR(verb.as_ptr()),
        PCWSTR(path.as_ptr()),
        PCWSTR(args.as_ptr()),
        PCWSTR::null(),
        SW_SHOWNORMAL,
    );
}
