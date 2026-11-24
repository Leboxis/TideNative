use std::collections::BTreeMap;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn main() {
    let exe_stem = env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_ascii_lowercase()));

    if exe_stem.as_deref() == Some("windres") {
        if let Err(err) = run_windres_shim() {
            eprintln!("tidenative windres shim error: {err}");
            std::process::exit(1);
        }
        return;
    }

    #[cfg(windows)]
    ensure_windres_on_path();

    tauri_build::build();

    #[cfg(windows)]
    if let Ok(out_dir) = env::var("OUT_DIR") {
        let lib_res = PathBuf::from(&out_dir).join("libresource.a");
        if lib_res.is_file() {
            let obj_path = PathBuf::from(&out_dir).join("tidenative_rsrc.o");
            if fs::copy(&lib_res, &obj_path).is_ok() {
                println!("cargo:rustc-link-arg-tests={}", obj_path.display());
            }
        }
    }
}

#[cfg(windows)]
fn ensure_windres_on_path() {
    if Command::new("windres")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
    {
        return;
    }

    let Ok(out_dir) = env::var("OUT_DIR") else {
        return;
    };
    let shim_dir = PathBuf::from(out_dir).join("windres-shim");
    if fs::create_dir_all(&shim_dir).is_err() {
        return;
    }
    let shim_exe = shim_dir.join("windres.exe");
    if let Ok(self_exe) = env::current_exe() {
        let _ = fs::copy(&self_exe, &shim_exe);
    }

    let existing_path = env::var_os("PATH").unwrap_or_default();
    let mut new_path = shim_dir.into_os_string();
    if !existing_path.is_empty() {
        new_path.push(";");
        new_path.push(existing_path);
    }
    // SAFETY: build script runs single-threaded before invoking tauri_build.
    unsafe {
        env::set_var("PATH", new_path);
    }
}

fn run_windres_shim() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-V" || a == "--version" || a == "/?") {
        println!("GNU windres (TideNative self-contained COFF resource compiler)");
        return Ok(());
    }

    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--input" | "-i" if i + 1 < args.len() => {
                input = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--output" | "-o" if i + 1 < args.len() => {
                output = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            _ => i += 1,
        }
    }

    let input = input.ok_or("missing --input argument")?;
    let output = output.ok_or("missing --output argument")?;

    let coff_bytes = if let Some(rc_exe) = find_windows_sdk_rc() {
        let res_path = output.with_extension("res");
        let status = Command::new(&rc_exe)
            .arg("/nologo")
            .arg("/c65001")
            .arg(format!("/fo{}", res_path.display()))
            .arg(&input)
            .status()
            .map_err(|e| format!("failed to run {}: {e}", rc_exe.display()))?;
        if !status.success() {
            return Err(format!("rc.exe exited with {status}"));
        }
        let res_bytes = fs::read(&res_path)
            .map_err(|e| format!("failed to read {}: {e}", res_path.display()))?;
        let entries = parse_res_file(&res_bytes)?;
        build_rsrc_coff_x64(&entries)?
    } else {
        return Err("Windows SDK rc.exe not found and windres is not in PATH".into());
    };

    // Write raw x86_64 COFF object directly (matching GNU windres --output-format=coff)
    // so the linker unconditionally merges the `.rsrc` section (manifest + icon + version).
    fs::write(&output, coff_bytes)
        .map_err(|e| format!("failed to write {}: {e}", output.display()))?;
    Ok(())
}

fn find_windows_sdk_rc() -> Option<PathBuf> {
    let kits_root = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let entries = fs::read_dir(kits_root).ok()?;
    let mut versions: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    versions.sort();
    for dir in versions.into_iter().rev() {
        let candidate = dir.join("x64").join("rc.exe");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let fallback = kits_root.join("x64").join("rc.exe");
    fallback.is_file().then_some(fallback)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ResId {
    Int(u32),
}

struct ResEntry {
    res_type: ResId,
    res_name: ResId,
    language: u32,
    data: Vec<u8>,
}

fn read_u16_le(buf: &[u8], off: usize) -> Result<u16, String> {
    let s = buf.get(off..off + 2).ok_or("unexpected EOF reading u16")?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

fn read_u32_le(buf: &[u8], off: usize) -> Result<u32, String> {
    let s = buf.get(off..off + 4).ok_or("unexpected EOF reading u32")?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn parse_res_id(buf: &[u8], mut off: usize) -> Result<(Option<ResId>, usize), String> {
    let first = read_u16_le(buf, off)?;
    if first == 0xFFFF {
        let id = read_u16_le(buf, off + 2)?;
        Ok((Some(ResId::Int(u32::from(id))), off + 4))
    } else {
        while read_u16_le(buf, off)? != 0 {
            off += 2;
        }
        Ok((None, off + 2))
    }
}

fn align4_usize(v: usize) -> usize {
    (v + 3) & !3
}

fn parse_res_file(buf: &[u8]) -> Result<Vec<ResEntry>, String> {
    let mut entries = Vec::new();
    let mut off = 0usize;
    while off + 8 <= buf.len() {
        let data_size = read_u32_le(buf, off)? as usize;
        let header_size = read_u32_le(buf, off + 4)? as usize;
        if header_size < 16 || off + header_size > buf.len() {
            break;
        }
        let mut cursor = off + 8;
        let (res_type, next) = parse_res_id(buf, cursor)?;
        cursor = next;
        let (res_name, next) = parse_res_id(buf, cursor)?;
        cursor = align4_usize(next);
        // Header tail: DataVersion (4) + MemoryFlags (2) + LanguageId (2) + Version (4) + Characteristics (4)
        let language = if cursor + 8 <= off + header_size {
            u32::from(read_u16_le(buf, cursor + 6)?)
        } else {
            0x0409
        };

        let data_start = off + header_size;
        let data_end = data_start + data_size;
        if data_end > buf.len() {
            return Err("truncated .res data payload".into());
        }
        // Skip the empty 32-byte leading null header record present in .res files.
        if data_size > 0 {
            if let (Some(rt), Some(rn)) = (res_type, res_name) {
                entries.push(ResEntry {
                    res_type: rt,
                    res_name: rn,
                    language,
                    data: buf[data_start..data_end].to_vec(),
                });
            }
        }
        off = align4_usize(data_end);
    }
    Ok(entries)
}

fn push_dir_header(out: &mut Vec<u8>, id_entries: u16) {
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&id_entries.to_le_bytes());
}

fn push_dir_entry(out: &mut Vec<u8>, id: u32, offset: u32, is_subdir: bool) {
    out.extend_from_slice(&id.to_le_bytes());
    let val = if is_subdir {
        offset | 0x8000_0000
    } else {
        offset
    };
    out.extend_from_slice(&val.to_le_bytes());
}

fn align4_vec(out: &mut Vec<u8>) {
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

fn build_rsrc_coff_x64(entries: &[ResEntry]) -> Result<Vec<u8>, String> {
    // Group: Type -> Name -> Language -> Data
    let mut tree: BTreeMap<u32, BTreeMap<u32, BTreeMap<u32, &[u8]>>> = BTreeMap::new();
    for entry in entries {
        let ResId::Int(rt) = entry.res_type;
        let ResId::Int(rn) = entry.res_name;
        tree.entry(rt)
            .or_default()
            .entry(rn)
            .or_default()
            .insert(entry.language, &entry.data);
    }

    // Calculate offsets for 3-level resource directory tree.
    let type_count = tree.len();
    let root_size = 16 + type_count * 8;

    let mut name_dir_offsets: BTreeMap<u32, u32> = BTreeMap::new();
    let mut cursor = root_size;
    for (&rt, names) in &tree {
        name_dir_offsets.insert(rt, cursor as u32);
        cursor += 16 + names.len() * 8;
    }

    let mut lang_dir_offsets: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for (&rt, names) in &tree {
        for (&rn, langs) in names {
            lang_dir_offsets.insert((rt, rn), cursor as u32);
            cursor += 16 + langs.len() * 8;
        }
    }

    let mut data_entry_offsets: BTreeMap<(u32, u32, u32), u32> = BTreeMap::new();
    let mut leaf_list: Vec<(u32, u32, u32, &[u8])> = Vec::new();
    for (&rt, names) in &tree {
        for (&rn, langs) in names {
            for (&lang, &data) in langs {
                data_entry_offsets.insert((rt, rn, lang), cursor as u32);
                leaf_list.push((rt, rn, lang, data));
                cursor += 16;
            }
        }
    }

    let off_raw_data_start = cursor;
    let mut rsrc = Vec::with_capacity(off_raw_data_start + 65_536);

    // Level 1: Root (Type) directory
    push_dir_header(&mut rsrc, type_count as u16);
    for (&rt, _) in &tree {
        push_dir_entry(&mut rsrc, rt, name_dir_offsets[&rt], true);
    }

    // Level 2: Name directories
    for (&rt, names) in &tree {
        push_dir_header(&mut rsrc, names.len() as u16);
        for (&rn, _) in names {
            push_dir_entry(&mut rsrc, rn, lang_dir_offsets[&(rt, rn)], true);
        }
    }

    // Level 3: Language directories
    for (&rt, names) in &tree {
        for (&rn, langs) in names {
            push_dir_header(&mut rsrc, langs.len() as u16);
            for (&lang, _) in langs {
                push_dir_entry(&mut rsrc, lang, data_entry_offsets[&(rt, rn, lang)], false);
            }
        }
    }

    // Reserve Data Entries
    rsrc.resize(off_raw_data_start, 0);

    // Append raw data blobs and fill IMAGE_RESOURCE_DATA_ENTRY + ADDR32NB relocations.
    let mut relocs: Vec<u32> = Vec::with_capacity(leaf_list.len());
    for (rt, rn, lang, data) in leaf_list {
        align4_vec(&mut rsrc);
        let blob_off = rsrc.len() as u32;
        rsrc.extend_from_slice(data);

        let de_off = data_entry_offsets[&(rt, rn, lang)] as usize;
        rsrc[de_off..de_off + 4].copy_from_slice(&blob_off.to_le_bytes());
        rsrc[de_off + 4..de_off + 8].copy_from_slice(&(data.len() as u32).to_le_bytes());
        rsrc[de_off + 8..de_off + 12].copy_from_slice(&0u32.to_le_bytes());
        rsrc[de_off + 12..de_off + 16].copy_from_slice(&0u32.to_le_bytes());
        relocs.push(de_off as u32);
    }
    align4_vec(&mut rsrc);

    // Assemble x86_64 COFF object file.
    let raw_data_ptr = 20u32 + 40u32;
    let reloc_ptr = raw_data_ptr + (rsrc.len() as u32);
    let symtab_ptr = reloc_ptr + (relocs.len() as u32) * 10;

    let mut obj = Vec::with_capacity(symtab_ptr as usize + 22);
    obj.extend_from_slice(&0x8664u16.to_le_bytes()); // AMD64
    obj.extend_from_slice(&1u16.to_le_bytes()); // NumberOfSections
    obj.extend_from_slice(&0u32.to_le_bytes());
    obj.extend_from_slice(&symtab_ptr.to_le_bytes());
    obj.extend_from_slice(&1u32.to_le_bytes());
    obj.extend_from_slice(&0u16.to_le_bytes());
    obj.extend_from_slice(&0u16.to_le_bytes());

    // .rsrc section header
    obj.extend_from_slice(b".rsrc\0\0\0");
    obj.extend_from_slice(&0u32.to_le_bytes());
    obj.extend_from_slice(&0u32.to_le_bytes());
    obj.extend_from_slice(&(rsrc.len() as u32).to_le_bytes());
    obj.extend_from_slice(&raw_data_ptr.to_le_bytes());
    obj.extend_from_slice(&reloc_ptr.to_le_bytes());
    obj.extend_from_slice(&0u32.to_le_bytes());
    obj.extend_from_slice(&(relocs.len() as u16).to_le_bytes());
    obj.extend_from_slice(&0u16.to_le_bytes());
    obj.extend_from_slice(&0x4000_0040u32.to_le_bytes());

    obj.extend_from_slice(&rsrc);

    for rva_off in relocs {
        obj.extend_from_slice(&rva_off.to_le_bytes());
        obj.extend_from_slice(&0u32.to_le_bytes());
        obj.extend_from_slice(&3u16.to_le_bytes()); // IMAGE_REL_AMD64_ADDR32NB
    }

    obj.extend_from_slice(b".rsrc\0\0\0");
    obj.extend_from_slice(&0u32.to_le_bytes());
    obj.extend_from_slice(&1i16.to_le_bytes());
    obj.extend_from_slice(&0u16.to_le_bytes());
    obj.push(3u8);
    obj.push(0u8);
    obj.extend_from_slice(&4u32.to_le_bytes());

    let _ = OsStr::new("");
    Ok(obj)
}
