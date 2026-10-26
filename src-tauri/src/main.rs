#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_icon;
mod backup;
mod bluetooth;
mod black_screen;
mod brightness;
mod commands;
mod data_folders;
mod dock_extras;
mod foreground;
mod hang_guard;
mod integrations;
mod jumplist;
mod later_windows;
mod launcher;
mod maintenance;
mod live_preview;
mod localsend;
mod localsend_receive;
mod notifications;
mod quick_settings;
mod recycle_bin;
mod services;
mod shelf;
mod shelf_preview;
mod snap_borders;
mod startup;
mod state;
mod taskbar_progress;
mod toast;
mod tray;
mod tray_host;
mod tray_manager;
mod types;
mod updater;
mod utils;
mod wifi;
mod window_capture;
mod window_memory;

use std::sync::atomic::Ordering;
use tauri::Manager;
use windows::core::BOOL;
use windows::Win32::System::Console::SetConsoleCtrlHandler;
use windows::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT};

use crate::commands::*;
use crate::services::*;
use crate::state::*;
use crate::utils::*;

unsafe extern "system" fn ctrl_handler(ctrl_type: u32) -> BOOL {
    if ctrl_type == CTRL_C_EVENT || ctrl_type == CTRL_BREAK_EVENT || ctrl_type == CTRL_CLOSE_EVENT {
        set_taskbar_visibility(true, true);
        NATIVE_TASKBAR_HIDDEN.store(false, Ordering::Relaxed);
        set_native_osd_suppressed(false);
        notifications::restore_native_toasts();
        taskbar_progress::restore();
        snap_borders::stop();
    }
    BOOL(0)
}

fn main() {
    // What went wrong, in crash.log (Bloom's data folder), before a panic
    // ends a thread or, inside a Windows callback, all of Bloom.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        utils::note_crash(&format!(
            "panic on thread '{}': {}",
            thread.name().unwrap_or("unnamed"),
            info.to_string().replace('\n', " ")
        ));
        default_hook(info);
    }));
    let mut args = std::env::args_os();
    let first = args.nth(1);
    if first.as_deref() == Some(std::ffi::OsStr::new("--taskbar-watchdog")) {
        if let (Some(pid), Some(marker)) = (args.next(), args.next()) {
            if let Ok(pid) = pid.to_string_lossy().parse::<u32>() {
                utils::run_taskbar_watchdog(pid, marker.into());
            }
        }
        return;
    }
    // The uninstaller: hand Windows back and leave.
    if first.as_deref() == Some(std::ffi::OsStr::new(startup::RESTORE_ARG)) {
        startup::restore_windows();
        return;
    }
    // Recovery, repair, enable/disable, diagnostics, the update watcher.
    if let Some(arg) = first.as_deref() {
        let rest: Vec<std::ffi::OsString> = args.by_ref().collect();
        if maintenance::run_command(arg, &rest) {
            return;
        }
    }
    // Started by the startup task as Windows signs in.
    let autostart = first.as_deref() == Some(std::ffi::OsStr::new(startup::AUTOSTART_ARG));
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(ctrl_handler), true);
    }

    // Single-instance enforcement
    unsafe {
        use windows::Win32::Foundation::{CloseHandle, GetLastError};
        use windows::Win32::System::Threading::{
            CreateEventW, CreateMutexW, OpenEventW, SetEvent, SYNCHRONIZATION_ACCESS_RIGHTS,
        };

        let mutex_name: Vec<u16> = "BloomSingleInstance"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let event_name: Vec<u16> = "BloomOpenSettings"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        let h_mutex = CreateMutexW(None, true, windows::core::PCWSTR(mutex_name.as_ptr())).ok();
        let err = GetLastError();

        if err.0 == 183 && autostart {
            // Already running (started some other way): nothing to open.
            if let Some(h) = h_mutex {
                let _ = CloseHandle(h);
            }
            return;
        }

        if err.0 == 183 {
            // Another instance is already running — signal it to open settings
            if let Ok(h_event) = OpenEventW(
                SYNCHRONIZATION_ACCESS_RIGHTS(0x00100002),
                false,
                windows::core::PCWSTR(event_name.as_ptr()),
            ) {
                let _ = SetEvent(h_event);
                let _ = CloseHandle(h_event);
            }
            if let Some(h) = h_mutex {
                let _ = CloseHandle(h);
            }
            return;
        }

        if let Ok(h_event) = CreateEventW(
            None,
            false,
            false,
            windows::core::PCWSTR(event_name.as_ptr()),
        ) {
            let _ = SINGLE_INSTANCE_EVENT_HANDLE.set(h_event.0 as isize);
        }
        if let Some(h) = h_mutex {
            let _ = SINGLE_INSTANCE_MUTEX_HANDLE.set(h.0 as isize);
        }
    }

    // Files kept under the original project's name move to Bloom's own,
    // before anything opens them.
    data_folders::move_old_data();

    if autostart {
        startup::boost_startup();
    }

    brightness::setup_worker();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .invoke_handler(tauri::generate_handler![
            hide_native_osd,
            open_settings_window,
            open_settings_tab,
            open_wifi_settings,
            open_sound_settings,
            open_notification_center,
            open_windows_notification_center,
            open_system_tray,
            set_ignore_cursor_events,
            set_window_height,
            resize_settings_window,
            hide_overlay,
            set_splash_fullscreen,
            sync_overlay_position,
            media_play_pause,
            media_next,
            media_previous,
            media_seek,
            open_media_source_app,
            init_dock,
            toggle_dock,
            change_dock_mode,
            change_notch_mode,
            sync_appbar,
            open_app,
            launch_new_instance,
            update_dock_rect,
            update_notch_rect,
            set_dock_hovered,
            set_notch_hovered,
            set_notch_open,
            dock_page_ready,
            later_windows::ensure_window,
            set_notch_visible,
            setup_shelf_drop,
            shelf_thumbnail,
            shelf_drag_icon,
            shelf_prune,
            shelf_open,
            get_active_windows,
            get_tray_apps,
            get_tray_state,
            double_click_tray_icon,
            startup::startup_enabled,
            startup::startup_set,
            show_tray_context_menu,
            activate_tray_icon,
            open_system_action,
            get_app_icon,
            get_installed_apps,
            save_pinned_apps,
            load_pinned_apps,
            clear_icon_cache,
            set_custom_icon,
            remove_custom_icon,
            get_custom_icons,
            set_menu_open,
            focus_window,
            switch_to_window,
            black_screen::screen_off_now,
            launcher::app_hotkey_status,
            launcher::set_hotkey_recording,
            maintenance::diagnostics_export,
            maintenance::bloom_repair,
            bluetooth::bluetooth_battery_refresh,
            focus_app_windows,
            close_window,
            quit_bloom,
            restart_bloom,
            get_volume,
            get_volume_state,
            get_brightness,
            set_volume,
            get_audio_sessions,
            set_app_volume,
            set_app_mute,
            set_volume_mixer_rect,
            clear_volume_mixer_rect,
            save_setting,
            load_settings,
            settings_file_readable,
            capture_window_thumbnail,
            get_wifi_status,
            set_wifi_state,
            get_bluetooth_state,
            set_bluetooth_state,
            open_bluetooth_settings,
            open_airplane_mode_settings,
            set_brightness,
            get_battery_saver_state,
            open_battery_saver_settings,
            get_system_accent_color,
            get_cpu_usage,
            get_ram_usage,
            get_disk_space,
            get_network_speed,
            export_settings,
            backup::export_backup,
            backup::import_backup,
            import_settings,
            reset_settings,
            read_settings_from_path,
            write_settings_to_path,
            updater::check_for_updates,
            updater::install_update,
            updater::get_update_state,
            data_folders::open_bloom_folder,
            jumplist::get_jump_list,
            jumplist::open_jump_item,
            launcher::launcher_hotkey_status,
            launcher::launcher_hide,
            launcher::launcher_set_size,
            launcher::launcher_set_game_mode,
            launcher::launcher_game_mode,
            launcher::launcher_open_settings,
            launcher::launcher_reindex,
            launcher::launcher_open_config_dir,
            launcher::actions::launcher_open,
            launcher::actions::launcher_run_command_line,
            launcher::actions::launcher_reveal,
            launcher::actions::launcher_copy_files,
            launcher::actions::launcher_recycle,
            launcher::actions::launcher_run_shell,
            launcher::actions::launcher_file_info,
            launcher::actions::launcher_list_dir,
            launcher::actions::launcher_path_programs,
            launcher::actions::launcher_resolve_app_path,
            launcher::bookmarks::launcher_bookmarks,
            launcher::icons::launcher_file_icon,
            launcher::icons::launcher_thumbnail,
            launcher::index::launcher_search_index,
            launcher::suggest::launcher_web_suggestions,
            launcher::system::launcher_system_command,
            launcher::system::launcher_processes,
            launcher::system::launcher_kill,
            dock_extras::get_dock_extras,
            dock_extras::open_dock_extra,
            dock_extras::eject_dock_extra,
            dock_extras::get_recycle_bin_extra,
            recycle_bin::empty_recycle_bin,
            end_task,
            live_preview::dock_live_previews,
            window_memory::forget_window_sizes,
            quick_settings::quick_settings_state,
            quick_settings::quick_setting_action,
            notifications::notifications_status,
            integrations::blitzit::blitzit_status,
            integrations::blitzit::blitzit_connect,
            integrations::blitzit::blitzit_cancel_connect,
            integrations::blitzit::blitzit_disconnect,
            integrations::blitzit::blitzit_lists,
            integrations::blitzit::blitzit_tasks,
            integrations::blitzit::blitzit_add_task,
            integrations::blitzit::blitzit_complete_task,
            integrations::blitzit::blitzit_start,
            integrations::blitzit::blitzit_open,
            integrations::blitzit::blitzit_tools_report,
            notifications::notification_activate,
            taskbar_progress::get_taskbar_progress,
            localsend::localsend_find_devices,
            localsend::localsend_stop_finding,
            localsend::localsend_send,
            localsend_receive::localsend_receive,
            localsend_receive::localsend_receive_status,
            localsend::localsend_cancel,
            notifications::notification_action,
            notifications::notification_card_closed,
            notifications::notification_center_fit,
            notifications::notifications_allow_typing,
            notifications::notifications_set_size,
            notifications::notifications_test,
            notifications::notifications_ready,
            settings_take_tab,
            notifications::notifications_open_privacy,
            notifications::notification_center_list,
            notifications::notification_remove,
            notifications::notification_clear_all,
            notifications::notification_center_hide,
            wifi::wifi_lists_nearby,
            wifi::wifi_list_networks,
            wifi::wifi_scan,
            wifi::wifi_connect,
            wifi::wifi_disconnect,
            wifi::wifi_forget,
            wifi::wifi_open_settings,
            bluetooth::bluetooth_list_devices,
            bluetooth::bluetooth_set_connected,
            bluetooth::bluetooth_discovery,
            bluetooth::bluetooth_pair,
            bluetooth::bluetooth_pairing_respond,
            bluetooth::bluetooth_unpair
        ])
        .setup(|app| {
            init_taskbar_marker(app.handle());
            // Populate the settings cache before anything else: a webview can
            // invoke save_setting as soon as it loads, which happens before the
            // rest of this hook runs.
            crate::utils::init_settings_cache(app.handle());
            // Crash-recovery: if a previous session was force-killed while the native
            // taskbar was hidden, restore it now. Runs before the frontend re-hides it
            // (init_dock fires after a delay), so the flag must be removed first.
            // A marker means the Bloom before left Explorer's taskbar hidden:
            // on purpose (sign-out, restart) or by a crash. It is taken over
            // as it is, so Windows' taskbar never shows in between; only a
            // marker from an old version is put back first.
            if taskbar_marker_exists() {
                if adopt_taskbar_marker() {
                    if dock_enabled() {
                        TASKBAR_TAKEOVER.store(true, Ordering::Relaxed);
                        if set_taskbar_visibility(false, false) {
                            NATIVE_TASKBAR_HIDDEN.store(true, Ordering::Relaxed);
                        }
                    } else {
                        set_taskbar_visibility(true, true);
                        NATIVE_TASKBAR_HIDDEN.store(false, Ordering::Relaxed);
                    }
                } else {
                    restore_taskbar_after_crash();
                    NATIVE_TASKBAR_HIDDEN.store(false, Ordering::Relaxed);
                }
            }

            let window = app.get_webview_window("main").unwrap();
            let dock_win = app.get_webview_window("dock").unwrap();

            // Sync window rects initially and on event
            let win_clone = window.clone();
            let update_main_rect = move || {
                if let (Ok(p), Ok(s)) = (win_clone.outer_position(), win_clone.outer_size()) {
                    if let Ok(mut lock) = MAIN_WINDOW_RECT.lock() {
                        *lock = Some((p, s));
                    }
                }
            };

            let dock_clone = dock_win.clone();
            let update_dock_window_rect = move || {
                if let (Ok(p), Ok(s)) = (dock_clone.outer_position(), dock_clone.outer_size()) {
                    if let Ok(mut lock) = DOCK_WINDOW_RECT.lock() {
                        *lock = Some((p, s));
                    }
                }
            };

            update_main_rect();
            update_dock_window_rect();

            let u_main = update_main_rect.clone();
            let handle_for_events = app.handle().clone();
            window.on_window_event(move |e| match e {
                tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                    u_main();
                    sync_overlays(&handle_for_events);
                }
                tauri::WindowEvent::ScaleFactorChanged { .. } => {
                    // Re-evaluate against the configured notch mode instead of
                    // registering unconditionally: on wake / display re-init a
                    // scale change in smart/peek mode must not reserve the top
                    // strip (issue #109).
                    let h = handle_for_events.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        reconcile_main_appbar(&h);
                        sync_overlays(&h);
                    });
                }
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    restore_taskbar_and_exit(&handle_for_events);
                }
                _ => {}
            });

            let u_dock = update_dock_window_rect.clone();
            let dock_for_events = dock_win.clone();
            let handle_for_dock_events = app.handle().clone();
            dock_win.on_window_event(move |e| match e {
                tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                    u_dock();
                    sync_overlays(&handle_for_dock_events);
                }
                tauri::WindowEvent::ScaleFactorChanged { .. } => {
                    let h = handle_for_dock_events.clone();
                    if DOCK_APPBAR_REGISTERED.load(Ordering::Relaxed) {
                        let w = dock_for_events.clone();
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                            register_dock_appbar(w);
                            sync_overlays(&h);
                        });
                    } else {
                        sync_overlays(&h);
                    }
                }
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    restore_taskbar_and_exit(&handle_for_dock_events);
                }
                _ => {}
            });

            sync_overlays(app.handle());

            // The overlay, notifications, notification center, search bar and
            // Settings are made once the notch is on screen (later_windows),
            // which sets each one up as it is made.
            watch_webview_processes(app.handle());
            later_windows::start(app.handle().clone());
            setup_mouse_hook(app.handle().clone());
            setup_display_change_monitor(app.handle().clone());
            setup_window_change_hook(app.handle().clone());
            {
                let _ = crate::state::THUMBNAIL_CACHE
                    .set(std::sync::Mutex::new(std::collections::HashMap::new()));
                let _ = crate::state::FOCUS_TIMESTAMPS
                    .set(std::sync::Mutex::new(std::collections::HashMap::new()));
                // Initialize before the scan so its results are actually stored.
                let _ = crate::state::INSTALLED_APPS_CACHE.set(std::sync::Mutex::new(Vec::new()));
            }
            setup_thumbnail_capture(app.handle().clone());
            // The installed-apps list (for the search bar) after the start.
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_secs(4));
                trigger_app_scan();
            });
            let tx = setup_system_worker(app.handle().clone());
            let _ = COMMAND_SENDER.set(tx.clone());
            services::setup_keyboard_hook(app.handle().clone());
            launcher::start_hotkey_listener(app.handle().clone());
            notifications::start(app.handle().clone());
            tray_host::start(app.handle().clone());
            hang_guard::start(app.handle().clone());
            snap_borders::apply(app.handle());
            startup::listen_for_quit(app.handle().clone());
            startup::migrate(app.handle());
            startup::clean_update_watchers();
            maintenance::repair_after_windows_update();
            window_memory::start(app.handle().clone());
            setup_taskbar_hook();
            setup_audio_visualization(app.handle().clone());
            setup_settings_watcher(app.handle().clone());
            wifi::setup_wifi_notifications(app.handle().clone());
            bluetooth::setup_bluetooth_notifications(app.handle().clone());
            bluetooth::start_battery_monitor(app.handle().clone());

            // Listen for second-instance signal to open settings
            if let Some(&h_event) = SINGLE_INSTANCE_EVENT_HANDLE.get() {
                if h_event != 0 {
                    let app_handle = app.handle().clone();
                    std::thread::spawn(move || {
                        use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
                        use windows::Win32::System::Threading::{WaitForSingleObject, INFINITE};
                        let h_event = HANDLE(h_event as *mut _);
                        loop {
                            let result = unsafe { WaitForSingleObject(h_event, INFINITE) };
                            if result == WAIT_OBJECT_0 {
                                crate::commands::open_settings_window(app_handle.clone());
                            }
                        }
                    });
                }
            }

            {
                use tauri::menu::{Menu, MenuItem};
                use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
                let quit_item = MenuItem::with_id(app, "quit", "Quit Bloom", true, None::<&str>)?;
                let restart_item =
                    MenuItem::with_id(app, "restart", "Restart Bloom", true, None::<&str>)?;
                let settings_item =
                    MenuItem::with_id(app, "settings", "Open Settings", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&settings_item, &restart_item, &quit_item])?;
                let ah = app.handle().clone();
                TrayIconBuilder::new()
                    .icon(app.default_window_icon().unwrap().clone())
                    .tooltip("Bloom")
                    .menu(&menu)
                    .on_menu_event(move |_, event| match event.id().as_ref() {
                        "quit" => {
                            SHUTTING_DOWN.store(true, Ordering::Relaxed);
                            if let Some(w) = ah.get_webview_window("main") {
                                unregister_appbar_native(w.hwnd().unwrap());
                            }
                            if let Some(w) = ah.get_webview_window("dock") {
                                unregister_appbar_native(w.hwnd().unwrap());
                            }
                            set_taskbar_visibility(true, true);
                            NATIVE_TASKBAR_HIDDEN.store(false, Ordering::Relaxed);
                            set_native_osd_suppressed(false);

                            ah.exit(0);
                        }
                        "restart" => {
                            SHUTTING_DOWN.store(true, Ordering::Relaxed);
                            if let Some(w) = ah.get_webview_window("main") {
                                unregister_appbar_native(w.hwnd().unwrap());
                            }
                            if let Some(w) = ah.get_webview_window("dock") {
                                unregister_appbar_native(w.hwnd().unwrap());
                            }
                            if let Some(w) = ah.get_webview_window("settings") {
                                let _ = w.destroy();
                            }
                            set_taskbar_visibility(true, true);
                            NATIVE_TASKBAR_HIDDEN.store(false, Ordering::Relaxed);
                            set_native_osd_suppressed(false);
                            notifications::restore_native_toasts();
                            close_single_instance_handles();

                            ah.restart();
                        }
                        "settings" => {
                            crate::commands::open_settings_window(ah.clone());
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            crate::commands::open_settings_window(tray.app_handle().clone());
                        }
                    })
                    .build(app)?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");
    app.run(|_, event| {
        if let tauri::RunEvent::Exit = event {
            // Signing out or shutting down: the taskbar stays in auto-hide and
            // hidden for the next sign-in (see `hand_over_taskbar`).
            if session_ending() {
                hand_over_taskbar("session-end");
            }
            if !HANDING_OVER.load(Ordering::Relaxed) {
                set_taskbar_visibility(true, true);
            }
            NATIVE_TASKBAR_HIDDEN.store(false, Ordering::Relaxed);
            set_native_osd_suppressed(false);
            notifications::restore_native_toasts();
        }
    });
}
