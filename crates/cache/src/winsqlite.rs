//! Liaison dynamique légère vers la bibliothèque système Windows `winsqlite3.dll`
//! (SQLite 3.x natif intégré à Windows 10/11 avec WAL, STRICT et FTS5, zéro coût binaire).

use crate::{CacheError, Result};
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;
use std::sync::OnceLock;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::core::{PCSTR, w};

pub const SQLITE_OK: c_int = 0;
pub const SQLITE_ROW: c_int = 100;
pub const SQLITE_DONE: c_int = 101;
pub const SQLITE_NULL: c_int = 5;

pub const SQLITE_OPEN_READWRITE: c_int = 0x0000_0002;
pub const SQLITE_OPEN_CREATE: c_int = 0x0000_0004;
pub const SQLITE_OPEN_FULLMUTEX: c_int = 0x0001_0000;

/// Destructeur `SQLITE_TRANSIENT` (`(void(*)(void*))-1`) indiquant à SQLite de copier la chaîne.
const SQLITE_TRANSIENT_VAL: isize = -1;

type Sqlite3 = c_void;
type Sqlite3Stmt = c_void;
type DestructorFn = Option<unsafe extern "system" fn(*mut c_void)>;

struct WinSqliteApi {
    open_v2: unsafe extern "system" fn(*const c_char, *mut *mut Sqlite3, c_int, *const c_char) -> c_int,
    close_v2: unsafe extern "system" fn(*mut Sqlite3) -> c_int,
    exec: unsafe extern "system" fn(
        *mut Sqlite3,
        *const c_char,
        *const c_void,
        *mut c_void,
        *mut *mut c_char,
    ) -> c_int,
    errmsg: unsafe extern "system" fn(*mut Sqlite3) -> *const c_char,
    prepare_v2: unsafe extern "system" fn(
        *mut Sqlite3,
        *const c_char,
        c_int,
        *mut *mut Sqlite3Stmt,
        *mut *const c_char,
    ) -> c_int,
    step: unsafe extern "system" fn(*mut Sqlite3Stmt) -> c_int,
    finalize: unsafe extern "system" fn(*mut Sqlite3Stmt) -> c_int,
    reset: unsafe extern "system" fn(*mut Sqlite3Stmt) -> c_int,
    clear_bindings: unsafe extern "system" fn(*mut Sqlite3Stmt) -> c_int,
    bind_int64: unsafe extern "system" fn(*mut Sqlite3Stmt, c_int, i64) -> c_int,
    bind_text: unsafe extern "system" fn(
        *mut Sqlite3Stmt,
        c_int,
        *const c_char,
        c_int,
        DestructorFn,
    ) -> c_int,
    bind_null: unsafe extern "system" fn(*mut Sqlite3Stmt, c_int) -> c_int,
    column_int64: unsafe extern "system" fn(*mut Sqlite3Stmt, c_int) -> i64,
    column_text: unsafe extern "system" fn(*mut Sqlite3Stmt, c_int) -> *const u8,
    column_bytes: unsafe extern "system" fn(*mut Sqlite3Stmt, c_int) -> c_int,
    column_type: unsafe extern "system" fn(*mut Sqlite3Stmt, c_int) -> c_int,
    libversion: unsafe extern "system" fn() -> *const c_char,
}

static API: OnceLock<std::result::Result<WinSqliteApi, String>> = OnceLock::new();

fn api() -> Result<&'static WinSqliteApi> {
    let loaded = API.get_or_init(|| unsafe {
        let module = LoadLibraryW(w!("winsqlite3.dll"))
            .map_err(|e| format!("impossible de charger winsqlite3.dll : {e}"))?;

        macro_rules! load_sym {
            ($name:literal) => {{
                let ptr = GetProcAddress(module, PCSTR(concat!($name, "\0").as_ptr()))
                    .ok_or_else(|| format!("symbole manquant dans winsqlite3.dll : {}", $name))?;
                std::mem::transmute(ptr)
            }};
        }

        Ok(WinSqliteApi {
            open_v2: load_sym!("sqlite3_open_v2"),
            close_v2: load_sym!("sqlite3_close_v2"),
            exec: load_sym!("sqlite3_exec"),
            errmsg: load_sym!("sqlite3_errmsg"),
            prepare_v2: load_sym!("sqlite3_prepare_v2"),
            step: load_sym!("sqlite3_step"),
            finalize: load_sym!("sqlite3_finalize"),
            reset: load_sym!("sqlite3_reset"),
            clear_bindings: load_sym!("sqlite3_clear_bindings"),
            bind_int64: load_sym!("sqlite3_bind_int64"),
            bind_text: load_sym!("sqlite3_bind_text"),
            bind_null: load_sym!("sqlite3_bind_null"),
            column_int64: load_sym!("sqlite3_column_int64"),
            column_text: load_sym!("sqlite3_column_text"),
            column_bytes: load_sym!("sqlite3_column_bytes"),
            column_type: load_sym!("sqlite3_column_type"),
            libversion: load_sym!("sqlite3_libversion"),
        })
    });

    match loaded {
        Ok(a) => Ok(a),
        Err(msg) => Err(CacheError::Sqlite(msg.clone())),
    }
}

/// Retourne la version exacte de `winsqlite3.dll` (ex. `"3.53.4"`).
pub fn sqlite_version() -> Result<String> {
    let a = api()?;
    let ptr = unsafe { (a.libversion)() };
    if ptr.is_null() {
        return Ok(String::new());
    }
    Ok(unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned())
}

pub struct Connection {
    raw: *mut Sqlite3,
    api: &'static WinSqliteApi,
}

// SAFETY: SQLite est ouvert avec `SQLITE_OPEN_FULLMUTEX` (mode serialized thread-safe)
// et `Connection` requiert `&mut self` pour toute mutation ou transaction.
unsafe impl Send for Connection {}

impl Connection {
    pub fn open_str(path_or_memory: &str) -> Result<Self> {
        let a = api()?;
        let c_path = CString::new(path_or_memory)
            .map_err(|_| CacheError::Sqlite("chemin SQLite contenant un octet nul".into()))?;
        let mut raw: *mut Sqlite3 = ptr::null_mut();
        let flags = SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_FULLMUTEX;
        let rc = unsafe { (a.open_v2)(c_path.as_ptr(), &mut raw, flags, ptr::null()) };
        if rc != SQLITE_OK || raw.is_null() {
            let msg = if !raw.is_null() {
                let err = unsafe { last_error_msg(a, raw) };
                unsafe { (a.close_v2)(raw) };
                err
            } else {
                format!("sqlite3_open_v2 rc={rc}")
            };
            return Err(CacheError::Sqlite(msg));
        }
        Ok(Self { raw, api: a })
    }

    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        let c_sql = CString::new(sql)
            .map_err(|_| CacheError::Sqlite("requête SQL contenant un octet nul".into()))?;
        let rc = unsafe {
            (self.api.exec)(
                self.raw,
                c_sql.as_ptr(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if rc != SQLITE_OK {
            return Err(CacheError::Sqlite(unsafe {
                last_error_msg(self.api, self.raw)
            }));
        }
        Ok(())
    }

    pub fn prepare(&self, sql: &str) -> Result<Statement<'_>> {
        let mut stmt: *mut Sqlite3Stmt = ptr::null_mut();
        let rc = unsafe {
            (self.api.prepare_v2)(
                self.raw,
                sql.as_ptr() as *const c_char,
                sql.len() as c_int,
                &mut stmt,
                ptr::null_mut(),
            )
        };
        if rc != SQLITE_OK || stmt.is_null() {
            return Err(CacheError::Sqlite(unsafe {
                last_error_msg(self.api, self.raw)
            }));
        }
        Ok(Statement {
            conn: self,
            raw: stmt,
        })
    }

    /// Exécute une closure à l'intérieur d'une transaction atomique (`BEGIN IMMEDIATE` / `COMMIT` / `ROLLBACK`).
    pub fn with_transaction<T>(&mut self, f: impl FnOnce(&Self) -> Result<T>) -> Result<T> {
        self.execute_batch("BEGIN IMMEDIATE;")?;
        match f(self) {
            Ok(val) => {
                self.execute_batch("COMMIT;")?;
                Ok(val)
            }
            Err(e) => {
                let _ = self.execute_batch("ROLLBACK;");
                Err(e)
            }
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                (self.api.close_v2)(self.raw);
            }
            self.raw = ptr::null_mut();
        }
    }
}

pub struct Statement<'c> {
    conn: &'c Connection,
    raw: *mut Sqlite3Stmt,
}

impl<'c> Statement<'c> {
    pub fn reset(&mut self) -> Result<()> {
        unsafe {
            (self.conn.api.reset)(self.raw);
            (self.conn.api.clear_bindings)(self.raw);
        }
        Ok(())
    }

    pub fn bind_i64(&mut self, index: i32, value: i64) -> Result<()> {
        let rc = unsafe { (self.conn.api.bind_int64)(self.raw, index, value) };
        self.check_rc(rc)
    }

    pub fn bind_str(&mut self, index: i32, value: &str) -> Result<()> {
        let transient: DestructorFn = unsafe { std::mem::transmute(SQLITE_TRANSIENT_VAL) };
        let rc = unsafe {
            (self.conn.api.bind_text)(
                self.raw,
                index,
                value.as_ptr() as *const c_char,
                value.len() as c_int,
                transient,
            )
        };
        self.check_rc(rc)
    }

    pub fn bind_opt_str(&mut self, index: i32, value: Option<&str>) -> Result<()> {
        match value {
            Some(s) => self.bind_str(index, s),
            None => {
                let rc = unsafe { (self.conn.api.bind_null)(self.raw, index) };
                self.check_rc(rc)
            }
        }
    }

    /// Exécute un statement DML (`INSERT`, `UPDATE`, `DELETE`) et le remet à zéro pour réutilisation.
    pub fn execute_and_reset(&mut self) -> Result<()> {
        let rc = unsafe { (self.conn.api.step)(self.raw) };
        let res = if rc == SQLITE_DONE || rc == SQLITE_ROW {
            Ok(())
        } else {
            Err(CacheError::Sqlite(unsafe {
                last_error_msg(self.conn.api, self.conn.raw)
            }))
        };
        let _ = self.reset();
        res
    }

    /// Avance vers la ligne suivante (`Ok(true)` = une ligne disponible, `Ok(false)` = fin).
    pub fn step_row(&mut self) -> Result<bool> {
        let rc = unsafe { (self.conn.api.step)(self.raw) };
        match rc {
            SQLITE_ROW => Ok(true),
            SQLITE_DONE => Ok(false),
            _ => Err(CacheError::Sqlite(unsafe {
                last_error_msg(self.conn.api, self.conn.raw)
            })),
        }
    }

    pub fn col_i64(&self, col: i32) -> i64 {
        unsafe { (self.conn.api.column_int64)(self.raw, col) }
    }

    pub fn col_str(&self, col: i32) -> String {
        self.col_opt_str(col).unwrap_or_default()
    }

    pub fn col_opt_str(&self, col: i32) -> Option<String> {
        let ty = unsafe { (self.conn.api.column_type)(self.raw, col) };
        if ty == SQLITE_NULL {
            return None;
        }
        let ptr = unsafe { (self.conn.api.column_text)(self.raw, col) };
        if ptr.is_null() {
            return None;
        }
        let len = unsafe { (self.conn.api.column_bytes)(self.raw, col) };
        if len <= 0 {
            return Some(String::new());
        }
        let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
        Some(String::from_utf8_lossy(slice).into_owned())
    }

    fn check_rc(&self, rc: c_int) -> Result<()> {
        if rc == SQLITE_OK {
            Ok(())
        } else {
            Err(CacheError::Sqlite(unsafe {
                last_error_msg(self.conn.api, self.conn.raw)
            }))
        }
    }
}

impl Drop for Statement<'_> {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                (self.conn.api.finalize)(self.raw);
            }
            self.raw = ptr::null_mut();
        }
    }
}

unsafe fn last_error_msg(a: &WinSqliteApi, db: *mut Sqlite3) -> String {
    let ptr = unsafe { (a.errmsg)(db) };
    if ptr.is_null() {
        return "erreur SQLite inconnue".into();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}
