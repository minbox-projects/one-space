//! Real macOS platform bridge, plus the stable unavailable fallback on other
//! targets.

/// Real macOS platform implementation using system frameworks through raw FFI.
#[cfg(target_os = "macos")]
mod mac {
    use super::super::controller::{BleLockKeychainItem, BleLockPlatform};
    use crate::ble_lock::BleLockError;
    use std::os::raw::{c_char, c_int, c_void};
    use std::ptr;
    use std::sync::OnceLock;
    use std::time::Duration;

    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFDataRef = *const c_void;
    type CFAllocatorRef = *const c_void;
    type CFDictionaryRef = *const c_void;
    type CFIndex = isize;
    type Boolean = u8;
    type OSStatus = i32;

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    /// `kCFNumberSInt32Type`.
    const K_CF_NUMBER_SINT32: c_int = 3;
    const ERR_SEC_SUCCESS: OSStatus = 0;
    const ERR_SEC_ITEM_NOT_FOUND: OSStatus = -25300;

    fn keychain_error() -> BleLockError {
        // Stable code only; never echoes a password or OS detail.
        BleLockError::new("keychain_unavailable", None)
    }

    fn native_unavailable() -> BleLockError {
        BleLockError::new("native_action_unavailable", None)
    }

    fn media_unavailable() -> BleLockError {
        BleLockError::new("media_control_unavailable", None)
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFBooleanTrue: CFTypeRef;
        static kCFBooleanFalse: CFTypeRef;
        static kCFTypeDictionaryKeyCallBacks: c_void;
        static kCFTypeDictionaryValueCallBacks: c_void;
        fn CFStringCreateWithBytes(
            alloc: CFAllocatorRef,
            bytes: *const u8,
            len: CFIndex,
            encoding: u32,
            is_external: Boolean,
        ) -> CFStringRef;
        fn CFDataCreate(alloc: CFAllocatorRef, bytes: *const u8, len: CFIndex) -> CFDataRef;
        fn CFDataGetBytePtr(data: CFDataRef) -> *const u8;
        fn CFDataGetLength(data: CFDataRef) -> CFIndex;
        fn CFDictionaryCreate(
            alloc: CFAllocatorRef,
            keys: *const *const c_void,
            values: *const *const c_void,
            count: CFIndex,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> CFDictionaryRef;
        fn CFDictionaryGetValue(dict: CFDictionaryRef, key: *const c_void) -> *const c_void;
        fn CFBooleanGetValue(value: CFTypeRef) -> Boolean;
        fn CFNumberGetValue(number: CFTypeRef, the_type: c_int, value: *mut c_void) -> Boolean;
        fn CFRelease(cf: CFTypeRef);
    }

    unsafe extern "C" {
        fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dispatch_queue_create(label: *const c_char, attr: *const c_void) -> *mut c_void;
        fn getuid() -> u32;
    }

    #[link(name = "Foundation", kind = "framework")]
    extern "C" {
        fn NSUserName() -> *const objc2_foundation::NSString;
    }

    #[link(name = "Security", kind = "framework")]
    extern "C" {
        static kSecClass: CFStringRef;
        static kSecClassGenericPassword: CFStringRef;
        static kSecAttrService: CFStringRef;
        static kSecAttrAccount: CFStringRef;
        static kSecAttrSynchronizable: CFStringRef;
        static kSecValueData: CFStringRef;
        static kSecReturnData: CFStringRef;
        fn SecItemAdd(attributes: CFDictionaryRef, result: *mut CFTypeRef) -> OSStatus;
        fn SecItemCopyMatching(query: CFDictionaryRef, result: *mut CFTypeRef) -> OSStatus;
        fn SecItemUpdate(query: CFDictionaryRef, attributes: CFDictionaryRef) -> OSStatus;
        fn SecItemDelete(query: CFDictionaryRef) -> OSStatus;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> Boolean;
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> Boolean;
        static kAXTrustedCheckOptionPrompt: CFStringRef;
        fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
    }

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPMAssertionDeclareUserActivity(
            assertion_name: CFStringRef,
            user_type: u32,
            assertion_id: *mut u32,
        ) -> i32;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreateKeyboardEvent(
            source: *const c_void,
            virtual_key: u16,
            key_down: bool,
        ) -> *mut c_void;
        fn CGEventKeyboardSetUnicodeString(event: *mut c_void, length: usize, string: *const u16);
        fn CGEventPost(tap: u32, event: *mut c_void);
    }

    type GetPlayingFn = unsafe extern "C" fn(*mut c_void, *mut c_void);
    type SendCommandFn = unsafe extern "C" fn(c_int, *const c_void) -> Boolean;

    const MEDIAREMOTE_PATH: &[u8] =
        b"/System/Library/PrivateFrameworks/MediaRemote.framework/MediaRemote\0";
    const LOGIN_FRAMEWORK_PATH: &[u8] =
        b"/System/Library/PrivateFrameworks/login.framework/login\0";
    const RTLD_LAZY: c_int = 0x1;

    fn cf_string(value: &str) -> CFStringRef {
        unsafe {
            CFStringCreateWithBytes(
                ptr::null(),
                value.as_ptr(),
                value.len() as CFIndex,
                K_CF_STRING_ENCODING_UTF8,
                0,
            )
        }
    }

    unsafe fn open_symbol(path: &[u8], symbol: &[u8]) -> Option<*mut c_void> {
        let handle = dlopen(path.as_ptr() as *const c_char, RTLD_LAZY);
        if handle.is_null() {
            return None;
        }
        let symbol = dlsym(handle, symbol.as_ptr() as *const c_char);
        if symbol.is_null() {
            None
        } else {
            Some(symbol)
        }
    }

    /// Build an owned CFDictionary from borrowed keys/values (caller releases).
    fn cf_dictionary(pairs: &[(*const c_void, *const c_void)]) -> Option<CFDictionaryRef> {
        unsafe {
            let keys: Vec<*const c_void> = pairs.iter().map(|(key, _)| *key).collect();
            let values: Vec<*const c_void> = pairs.iter().map(|(_, value)| *value).collect();
            let dictionary = CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                pairs.len() as CFIndex,
                &kCFTypeDictionaryKeyCallBacks as *const c_void,
                &kCFTypeDictionaryValueCallBacks as *const c_void,
            );
            if dictionary.is_null() {
                None
            } else {
                Some(dictionary)
            }
        }
    }

    fn keychain_query(
        item: &BleLockKeychainItem,
        include_value: Option<&str>,
    ) -> Option<(CFDictionaryRef, Vec<CFTypeRef>)> {
        unsafe {
            let service = cf_string(&item.service);
            let account = cf_string(&item.account);
            if service.is_null() || account.is_null() {
                if !service.is_null() {
                    CFRelease(service);
                }
                if !account.is_null() {
                    CFRelease(account);
                }
                return None;
            }
            let mut owned: Vec<CFTypeRef> = vec![service, account];
            let data = match include_value {
                Some(value) => {
                    let data = CFDataCreate(ptr::null(), value.as_ptr(), value.len() as CFIndex);
                    if data.is_null() {
                        for value in owned {
                            CFRelease(value);
                        }
                        return None;
                    }
                    owned.push(data);
                    data
                }
                None => ptr::null(),
            };

            let mut keys: Vec<*const c_void> = vec![
                kSecClass,
                kSecAttrService,
                kSecAttrAccount,
                kSecAttrSynchronizable,
            ];
            let mut values: Vec<*const c_void> =
                vec![kSecClassGenericPassword, service, account, kCFBooleanFalse];
            if include_value.is_some() {
                keys.push(kSecValueData);
                values.push(data);
            }
            let dictionary = CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                keys.len() as CFIndex,
                &kCFTypeDictionaryKeyCallBacks as *const c_void,
                &kCFTypeDictionaryValueCallBacks as *const c_void,
            );
            if dictionary.is_null() {
                for value in owned {
                    CFRelease(value);
                }
                return None;
            }
            Some((dictionary, owned))
        }
    }

    fn keychain_read(item: &BleLockKeychainItem) -> Result<Option<String>, BleLockError> {
        unsafe {
            let service = cf_string(&item.service);
            let account = cf_string(&item.account);
            if service.is_null() || account.is_null() {
                if !service.is_null() {
                    CFRelease(service);
                }
                if !account.is_null() {
                    CFRelease(account);
                }
                return Err(keychain_error());
            }
            let keys: [*const c_void; 5] = [
                kSecClass,
                kSecAttrService,
                kSecAttrAccount,
                kSecAttrSynchronizable,
                kSecReturnData,
            ];
            let values: [*const c_void; 5] = [
                kSecClassGenericPassword,
                service,
                account,
                kCFBooleanFalse,
                kCFBooleanTrue,
            ];
            let query = CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                5,
                &kCFTypeDictionaryKeyCallBacks as *const c_void,
                &kCFTypeDictionaryValueCallBacks as *const c_void,
            );
            let mut result: CFTypeRef = ptr::null();
            let status = if query.is_null() {
                -1
            } else {
                SecItemCopyMatching(query, &mut result)
            };
            if !query.is_null() {
                CFRelease(query);
            }
            CFRelease(service);
            CFRelease(account);
            if status == ERR_SEC_ITEM_NOT_FOUND {
                return Ok(None);
            }
            if status != ERR_SEC_SUCCESS || result.is_null() {
                if !result.is_null() {
                    CFRelease(result);
                }
                return Err(keychain_error());
            }
            let data = result as CFDataRef;
            let length = CFDataGetLength(data);
            let bytes = CFDataGetBytePtr(data);
            let value = if bytes.is_null() || length < 0 {
                Err(keychain_error())
            } else {
                let slice = std::slice::from_raw_parts(bytes, length as usize);
                String::from_utf8(slice.to_vec()).map_err(|_| keychain_error())
            };
            CFRelease(result);
            value.map(Some)
        }
    }

    fn keychain_write(item: &BleLockKeychainItem, password: &str) -> Result<(), BleLockError> {
        // Replace with `SecItemUpdate`, adding only when the item is absent, so
        // repeated saves never delete the existing item before success.
        unsafe {
            let service = cf_string(&item.service);
            let account = cf_string(&item.account);
            let data = CFDataCreate(ptr::null(), password.as_ptr(), password.len() as CFIndex);
            if service.is_null() || account.is_null() || data.is_null() {
                if !service.is_null() {
                    CFRelease(service);
                }
                if !account.is_null() {
                    CFRelease(account);
                }
                if !data.is_null() {
                    CFRelease(data);
                }
                return Err(keychain_error());
            }
            let owned = [service, account, data];

            let query = cf_dictionary(&[
                (kSecClass, kSecClassGenericPassword),
                (kSecAttrService, service),
                (kSecAttrAccount, account),
                (kSecAttrSynchronizable, kCFBooleanFalse),
            ]);
            let attributes = cf_dictionary(&[(kSecValueData, data)]);
            let update_status = match (query, attributes) {
                (Some(query), Some(attributes)) => {
                    let status = SecItemUpdate(query, attributes);
                    CFRelease(query);
                    CFRelease(attributes);
                    status
                }
                _ => -1,
            };
            if update_status == ERR_SEC_SUCCESS {
                for value in owned {
                    CFRelease(value);
                }
                return Ok(());
            }

            let add = cf_dictionary(&[
                (kSecClass, kSecClassGenericPassword),
                (kSecAttrService, service),
                (kSecAttrAccount, account),
                (kSecAttrSynchronizable, kCFBooleanFalse),
                (kSecValueData, data),
            ]);
            let add_status = if let Some(add) = add {
                let mut result: CFTypeRef = ptr::null();
                let status = SecItemAdd(add, &mut result);
                if !result.is_null() {
                    CFRelease(result);
                }
                CFRelease(add);
                status
            } else {
                -1
            };
            for value in owned {
                CFRelease(value);
            }
            if add_status == ERR_SEC_SUCCESS {
                Ok(())
            } else {
                Err(keychain_error())
            }
        }
    }

    fn keychain_delete(item: &BleLockKeychainItem) -> Result<(), BleLockError> {
        unsafe {
            let Some((query, owned)) = keychain_query(item, None) else {
                return Err(keychain_error());
            };
            let status = SecItemDelete(query);
            CFRelease(query);
            for value in owned {
                CFRelease(value);
            }
            if status == ERR_SEC_SUCCESS || status == ERR_SEC_ITEM_NOT_FOUND {
                Ok(())
            } else {
                Err(keychain_error())
            }
        }
    }

    /// The real OS user account name for this process (never `$USER`).
    fn current_user_account_name() -> Result<String, BleLockError> {
        objc2::rc::autoreleasepool(|_| {
            // Foundation returns an NSString, not a caller-sized passwd struct.
            // Copy its checked non-null result before the autorelease pool ends.
            let account = unsafe { NSUserName().as_ref() }.ok_or_else(keychain_error)?;
            let name = account.to_string();
            if name.is_empty() {
                Err(keychain_error())
            } else {
                Ok(name)
            }
        })
    }

    /// Observed lock of the *current ordinary logged-in console* session.
    ///
    /// False at loginwindow, pre-boot, for any non-console session and for a
    /// different OS user; the undocumented `CGSSessionScreenIsLocked` flag alone
    /// is never sufficient.
    fn session_locked() -> bool {
        unsafe {
            let dictionary = CGSessionCopyCurrentDictionary();
            if dictionary.is_null() {
                return false;
            }
            let on_console_key = cf_string("kCGSSessionOnConsoleKey");
            let locked_key = cf_string("CGSSessionScreenIsLocked");
            let uid_key = cf_string("kCGSSessionUserIDKey");
            let login_done_key = cf_string("kCGSessionLoginDoneKey");
            let bool_value = |key: CFStringRef| -> bool {
                if key.is_null() {
                    return false;
                }
                let value = CFDictionaryGetValue(dictionary, key);
                !value.is_null() && CFBooleanGetValue(value) != 0
            };
            let on_console = bool_value(on_console_key);
            let locked = bool_value(locked_key);
            let login_done = bool_value(login_done_key);
            let mut session_uid: c_int = -1;
            if !uid_key.is_null() {
                let value = CFDictionaryGetValue(dictionary, uid_key);
                if !value.is_null() {
                    CFNumberGetValue(
                        value,
                        K_CF_NUMBER_SINT32,
                        &mut session_uid as *mut c_int as *mut c_void,
                    );
                }
            }
            for key in [on_console_key, locked_key, uid_key, login_done_key] {
                if !key.is_null() {
                    CFRelease(key);
                }
            }
            CFRelease(dictionary);
            on_console && login_done && locked && session_uid >= 0 && session_uid as u32 == getuid()
        }
    }

    fn request_accessibility_trust() -> bool {
        unsafe {
            let key = kAXTrustedCheckOptionPrompt;
            let values: [*const c_void; 1] = [kCFBooleanTrue];
            let keys: [*const c_void; 1] = [key];
            let options = CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks as *const c_void,
                &kCFTypeDictionaryValueCallBacks as *const c_void,
            );
            if options.is_null() {
                return false;
            }
            let trusted = AXIsProcessTrustedWithOptions(options) != 0;
            CFRelease(options);
            trusted
        }
    }

    fn inject_password(password: &str) -> Result<(), BleLockError> {
        // CGEventKeyboardSetUnicodeString accepts at most 20 UTF-16 units per
        // event; never split a surrogate pair across events.
        const MAX_UNITS: usize = 20;
        let units: Vec<u16> = password.encode_utf16().collect();
        let mut index = 0;
        while index < units.len() {
            let mut end = (index + MAX_UNITS).min(units.len());
            if end < units.len() && (0xD800..=0xDBFF).contains(&units[end - 1]) {
                end -= 1;
            }
            if end <= index {
                end = (index + 1).min(units.len());
            }
            let chunk = &units[index..end];
            unsafe {
                let down = CGEventCreateKeyboardEvent(ptr::null(), 0, true);
                if down.is_null() {
                    return Err(native_unavailable());
                }
                CGEventKeyboardSetUnicodeString(down, chunk.len(), chunk.as_ptr());
                CGEventPost(0, down);
                CFRelease(down);
                let up = CGEventCreateKeyboardEvent(ptr::null(), 0, false);
                if up.is_null() {
                    return Err(native_unavailable());
                }
                CGEventPost(0, up);
                CFRelease(up);
            }
            index = end;
        }
        // Submit with one Return key press.
        unsafe {
            for key_down in [true, false] {
                let event = CGEventCreateKeyboardEvent(ptr::null(), 36, key_down);
                if event.is_null() {
                    return Err(native_unavailable());
                }
                CGEventPost(0, event);
                CFRelease(event);
            }
        }
        Ok(())
    }

    struct MediaSymbols {
        get_playing: GetPlayingFn,
        send_command: SendCommandFn,
    }

    /// Resolve the private MediaRemote capability once for the process lifetime.
    fn media_symbols() -> Option<&'static MediaSymbols> {
        static SYMBOLS: OnceLock<Option<MediaSymbols>> = OnceLock::new();
        SYMBOLS
            .get_or_init(|| unsafe {
                let handle = dlopen(MEDIAREMOTE_PATH.as_ptr() as *const c_char, RTLD_LAZY);
                if handle.is_null() {
                    return None;
                }
                let get_playing = dlsym(
                    handle,
                    b"MRMediaRemoteGetNowPlayingApplicationIsPlaying\0".as_ptr() as *const c_char,
                );
                let send_command = dlsym(
                    handle,
                    b"MRMediaRemoteSendCommand\0".as_ptr() as *const c_char,
                );
                if get_playing.is_null() || send_command.is_null() {
                    return None;
                }
                Some(MediaSymbols {
                    get_playing: std::mem::transmute(get_playing),
                    send_command: std::mem::transmute(send_command),
                })
            })
            .as_ref()
    }

    /// A single private completion queue for MediaRemote callbacks, so waiting
    /// never blocks the main queue.
    fn media_queue() -> Option<*mut c_void> {
        static QUEUE: OnceLock<usize> = OnceLock::new();
        let raw = *QUEUE.get_or_init(|| unsafe {
            dispatch_queue_create(
                b"com.onespace.ble-lock.media\0".as_ptr() as *const c_char,
                ptr::null(),
            ) as usize
        });
        if raw == 0 {
            None
        } else {
            Some(raw as *mut c_void)
        }
    }

    fn media_playing() -> Result<bool, BleLockError> {
        let Some(symbols) = media_symbols() else {
            return Err(media_unavailable());
        };
        let Some(queue) = media_queue() else {
            return Err(media_unavailable());
        };
        let (sender, receiver) = std::sync::mpsc::channel::<bool>();
        let block = block2::RcBlock::new(move |is_playing: u8| {
            let _ = sender.send(is_playing != 0);
        });
        unsafe {
            (symbols.get_playing)(queue, &*block as *const _ as *mut c_void);
        }
        match receiver.recv_timeout(Duration::from_millis(500)) {
            Ok(playing) => Ok(playing),
            Err(_) => Err(media_unavailable()),
        }
    }

    fn media_command(command: c_int) -> Result<(), BleLockError> {
        let Some(symbols) = media_symbols() else {
            return Err(media_unavailable());
        };
        if unsafe { (symbols.send_command)(command, ptr::null()) } != 0 {
            Ok(())
        } else {
            Err(media_unavailable())
        }
    }

    /// Real macOS platform.
    #[derive(Clone, Copy, Debug)]
    pub struct MacPlatform;

    impl MacPlatform {
        /// Query the current OS account without a handwritten Darwin ABI.
        pub fn current_account_name() -> Result<String, BleLockError> {
            current_user_account_name()
        }
    }

    impl BleLockPlatform for MacPlatform {
        fn current_user_account(&self) -> String {
            // The trait exposes a String; an unavailable name produces no usable
            // item account, and the item operations below return a stable error.
            Self::current_account_name().unwrap_or_default()
        }

        fn keychain_read(
            &self,
            item: &BleLockKeychainItem,
        ) -> Result<Option<String>, BleLockError> {
            if item.account.is_empty() {
                return Err(keychain_error());
            }
            keychain_read(item)
        }

        fn keychain_write(
            &self,
            item: &BleLockKeychainItem,
            password: &str,
        ) -> Result<(), BleLockError> {
            if item.account.is_empty() {
                return Err(keychain_error());
            }
            keychain_write(item, password)
        }

        fn keychain_delete(&self, item: &BleLockKeychainItem) -> Result<(), BleLockError> {
            if item.account.is_empty() {
                return Err(keychain_error());
            }
            keychain_delete(item)
        }

        fn accessibility_trusted(&self) -> bool {
            unsafe { AXIsProcessTrusted() != 0 }
        }

        fn is_session_locked(&self) -> bool {
            session_locked()
        }

        fn lock_session(&self) -> Result<(), BleLockError> {
            unsafe {
                let Some(symbol) = open_symbol(LOGIN_FRAMEWORK_PATH, b"SACLockScreenImmediate\0")
                else {
                    return Err(native_unavailable());
                };
                // Private login framework symbol is `void SACLockScreenImmediate(void)`.
                let lock: unsafe extern "C" fn() = std::mem::transmute(symbol);
                lock();
                Ok(())
            }
        }

        fn launch_screensaver(&self) -> Result<(), BleLockError> {
            let status = std::process::Command::new("/usr/bin/open")
                .args(["-a", "ScreenSaverEngine"])
                .status()
                .map_err(|_| native_unavailable())?;
            if status.success() {
                Ok(())
            } else {
                Err(native_unavailable())
            }
        }

        fn sleep_display(&self) -> Result<(), BleLockError> {
            let status = std::process::Command::new("/usr/bin/pmset")
                .arg("displaysleepnow")
                .status()
                .map_err(|_| native_unavailable())?;
            if status.success() {
                Ok(())
            } else {
                Err(native_unavailable())
            }
        }

        fn wake_display(&self) -> Result<(), BleLockError> {
            unsafe {
                let name = cf_string("OneSpace BLE Lock Wake");
                if name.is_null() {
                    return Err(native_unavailable());
                }
                let mut assertion_id: u32 = 0;
                let result = IOPMAssertionDeclareUserActivity(name, 0, &mut assertion_id);
                CFRelease(name);
                if result == 0 {
                    Ok(())
                } else {
                    Err(native_unavailable())
                }
            }
        }

        fn inject_password(&self, password: &str) -> Result<(), BleLockError> {
            inject_password(password)
        }

        fn media_playing(&self) -> Result<bool, BleLockError> {
            media_playing()
        }

        fn media_pause(&self) -> Result<(), BleLockError> {
            media_command(1)
        }

        fn media_play(&self) -> Result<(), BleLockError> {
            media_command(0)
        }
    }

    /// Explicitly request Accessibility trust (asynchronous system prompt).
    pub fn request_accessibility() -> bool {
        request_accessibility_trust()
    }
}

/// Non-macOS platform: every native capability reports a stable unavailable
/// error and no OS action is ever attempted.
#[cfg(not(target_os = "macos"))]
mod mac {
    use super::super::controller::{BleLockKeychainItem, BleLockPlatform};
    use crate::ble_lock::BleLockError;

    /// Unavailable platform used on Windows/Linux and non-Tauri previews.
    #[derive(Clone, Copy, Debug)]
    pub struct MacPlatform;

    fn unavailable() -> BleLockError {
        BleLockError::platform_unavailable()
    }

    impl MacPlatform {
        pub fn current_account_name() -> Result<String, BleLockError> {
            Err(unavailable())
        }
    }

    impl BleLockPlatform for MacPlatform {
        fn current_user_account(&self) -> String {
            Self::current_account_name().unwrap_or_default()
        }

        fn keychain_read(
            &self,
            _item: &BleLockKeychainItem,
        ) -> Result<Option<String>, BleLockError> {
            Err(unavailable())
        }

        fn keychain_write(
            &self,
            _item: &BleLockKeychainItem,
            _password: &str,
        ) -> Result<(), BleLockError> {
            Err(unavailable())
        }

        fn keychain_delete(&self, _item: &BleLockKeychainItem) -> Result<(), BleLockError> {
            Err(unavailable())
        }

        fn accessibility_trusted(&self) -> bool {
            false
        }

        fn is_session_locked(&self) -> bool {
            false
        }

        fn lock_session(&self) -> Result<(), BleLockError> {
            Err(unavailable())
        }

        fn launch_screensaver(&self) -> Result<(), BleLockError> {
            Err(unavailable())
        }

        fn sleep_display(&self) -> Result<(), BleLockError> {
            Err(unavailable())
        }

        fn wake_display(&self) -> Result<(), BleLockError> {
            Err(unavailable())
        }

        fn inject_password(&self, _password: &str) -> Result<(), BleLockError> {
            Err(unavailable())
        }

        fn media_playing(&self) -> Result<bool, BleLockError> {
            Err(BleLockError::new("media_control_unavailable", None))
        }

        fn media_pause(&self) -> Result<(), BleLockError> {
            Err(BleLockError::new("media_control_unavailable", None))
        }

        fn media_play(&self) -> Result<(), BleLockError> {
            Err(BleLockError::new("media_control_unavailable", None))
        }
    }

    /// Non-macOS request is a no-op boolean.
    pub fn request_accessibility() -> bool {
        false
    }
}

pub use mac::MacPlatform;
pub(super) use mac::request_accessibility;
