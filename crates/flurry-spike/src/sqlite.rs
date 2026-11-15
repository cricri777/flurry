use libsqlite3_sys as ffi;
use std::ffi::{CStr, CString};
use std::ptr;

fn exec(db: *mut ffi::sqlite3, sql: &str) -> Result<(), String> {
    let sql = CString::new(sql).unwrap();
    let mut err = ptr::null_mut();
    let rc = unsafe { ffi::sqlite3_exec(db, sql.as_ptr(), None, ptr::null_mut(), &mut err) };
    if rc == ffi::SQLITE_OK {
        return Ok(());
    }
    let msg = unsafe { CStr::from_ptr(err).to_string_lossy().into_owned() };
    unsafe { ffi::sqlite3_free(err.cast()) };
    Err(msg)
}

pub fn run(path: &str) -> Result<i64, String> {
    let mut db = ptr::null_mut();
    let p = CString::new(path).unwrap();
    if unsafe { ffi::sqlite3_open(p.as_ptr(), &mut db) } != ffi::SQLITE_OK {
        return Err("open failed".into());
    }
    let result = (|| {
        exec(
            db,
            "CREATE TABLE IF NOT EXISTS spike (id INTEGER PRIMARY KEY, at TEXT NOT NULL)",
        )?;
        exec(db, "INSERT INTO spike (at) VALUES (datetime('now'))")?;
        let sql = CString::new("SELECT COUNT(*) FROM spike").unwrap();
        let mut stmt = ptr::null_mut();
        unsafe { ffi::sqlite3_prepare_v2(db, sql.as_ptr(), -1, &mut stmt, ptr::null_mut()) };
        unsafe { ffi::sqlite3_step(stmt) };
        let n = unsafe { ffi::sqlite3_column_int64(stmt, 0) };
        unsafe { ffi::sqlite3_finalize(stmt) };
        Ok(n)
    })();
    unsafe { ffi::sqlite3_close(db) };
    result
}
