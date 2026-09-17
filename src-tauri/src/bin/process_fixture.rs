//! Process fixtures used by Phase 5 association and close tests.
//!
//! Modes:
//! - `responsive` — visible window that exits on WM_CLOSE
//! - `ignore-close` — visible window that ignores WM_CLOSE
//! - `child` — launches another fixture mode then exits (intermediate launcher)
//! - `sleep` — no window; sleeps until killed

#![cfg(windows)]

use std::env;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostQuitMessage,
    RegisterClassW, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, MSG,
    SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_DESTROY, WNDCLASSW,
};

fn main() {
    let mode = env::args().nth(1).unwrap_or_else(|| "responsive".into());
    match mode.as_str() {
        "responsive" => run_window(true),
        "ignore-close" => run_window(false),
        "child" => run_child_launcher(),
        "sleep" => thread::sleep(Duration::from_secs(120)),
        other => {
            eprintln!("unknown process-fixture mode: {other}");
            std::process::exit(2);
        }
    }
}

fn run_child_launcher() {
    let self_exe = env::current_exe().expect("current exe");
    // Unique copies should launch the shared fixture binary as the "game".
    let shared = self_exe.with_file_name("process-fixture.exe");
    let game = if shared.is_file() { shared } else { self_exe };
    // Intentionally detach: the launcher exits while the game keeps running.
    #[allow(clippy::zombie_processes)]
    let _child = Command::new(game)
        .arg("responsive")
        .spawn()
        .expect("spawn child fixture");
}

fn run_window(exit_on_close: bool) {
    let class_name = w!("MediaDeckProcessFixture");
    let instance = unsafe { GetModuleHandleW(None) }.expect("module");
    let wnd = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(if exit_on_close {
            responsive_wnd_proc
        } else {
            ignore_wnd_proc
        }),
        hInstance: instance.into(),
        lpszClassName: class_name,
        ..Default::default()
    };
    unsafe { RegisterClassW(&wnd) };

    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("MediaDeck Fixture"),
            WINDOW_STYLE(0x00CF_0000), // WS_OVERLAPPEDWINDOW
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            320,
            200,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }
    .expect("create window");

    let _ = unsafe { ShowWindow(hwnd, SW_SHOW) };
    let running = Arc::new(AtomicBool::new(true));
    let flag = Arc::clone(&running);
    thread::spawn(move || {
        while flag.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(50));
        }
    });

    let mut message = MSG::default();
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.into() {
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    running.store(false, Ordering::SeqCst);
}

unsafe extern "system" fn responsive_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CLOSE | WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe extern "system" fn ignore_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CLOSE => LRESULT(0),
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
