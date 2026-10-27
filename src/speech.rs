use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct SpeechEngine {
    worker: Mutex<Option<Worker>>,
    loaded: AtomicBool,
}
struct Worker {
    child: Child,
    root: PathBuf,
    directory: tempfile::TempDir,
    #[cfg(windows)]
    _job: crate::local_ai::ProcessJob,
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl SpeechEngine {
    pub fn is_loaded(&self) -> bool {
        if self.loaded.load(Ordering::Acquire)
            && let Ok(mut worker) = self.worker.try_lock()
            && worker
                .as_mut()
                .is_none_or(|w| w.child.try_wait().ok() != Some(None))
        {
            *worker = None;
            self.loaded.store(false, Ordering::Release);
        }
        self.loaded.load(Ordering::Acquire)
    }
    pub fn stop(&self) {
        if let Ok(mut worker) = self.worker.lock() {
            *worker = None;
        }
        self.loaded.store(false, Ordering::Release);
    }
    pub fn warmup(&self, root: &Path, voice: u32, cancel: &AtomicBool) -> Result<(), String> {
        self.request(
            root,
            serde_json::json!({"action":"warmup", "voice":voice}),
            cancel,
        )
    }
    fn request(
        &self,
        root: &Path,
        request: serde_json::Value,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        if cancel.load(Ordering::Relaxed) {
            return Err("已停止朗读".into());
        }
        if !ready(root) {
            return Err("请先在语音设置中下载 Kokoro 语音模型".into());
        }
        let mut guard = self.worker.lock().map_err(|_| "语音进程状态不可用")?;
        if guard
            .as_mut()
            .is_some_and(|w| w.root != root || w.child.try_wait().ok() != Some(None))
        {
            *guard = None;
            self.loaded.store(false, Ordering::Release);
        }
        if guard.is_none() {
            let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
            let script = directory.path().join("kokoro-worker.py");
            fs::write(&script, include_str!("../scripts/kokoro-worker.py"))
                .map_err(|e| e.to_string())?;
            let log =
                fs::File::create(directory.path().join("worker.log")).map_err(|e| e.to_string())?;
            let mut command = Command::new(root.join("python/python.exe"));
            command
                .arg(script)
                .arg("--serve")
                .arg(root.join("kokoro"))
                .arg(directory.path())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(log);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            let mut child = command.spawn().map_err(|e| e.to_string())?;
            #[cfg(windows)]
            let job = crate::local_ai::ProcessJob::attach(&child).map_err(|e| {
                let _ = child.kill();
                let _ = child.wait();
                e.to_string()
            })?;
            *guard = Some(Worker {
                child,
                root: root.into(),
                directory,
                #[cfg(windows)]
                _job: job,
            });
        }
        let result = (|| {
            let worker = guard.as_mut().unwrap();
            let directory = worker.directory.path();
            let temporary = directory.join("request.tmp");
            fs::write(
                &temporary,
                serde_json::to_vec(&request).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            fs::rename(temporary, directory.join("request.json")).map_err(|e| e.to_string())?;
            let started = Instant::now();
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err("已停止朗读".into());
                }
                if started.elapsed() > Duration::from_secs(120) {
                    return Err("语音生成超时，请重试".into());
                }
                let response = directory.join("response.json");
                if response.is_file() {
                    let value: serde_json::Value =
                        serde_json::from_slice(&fs::read(&response).map_err(|e| e.to_string())?)
                            .map_err(|e| e.to_string())?;
                    fs::remove_file(response).map_err(|e| e.to_string())?;
                    if value["ok"] == true {
                        return Ok(());
                    }
                    return Err(format!(
                        "语音生成失败：{}",
                        value["error"].as_str().unwrap_or("未知错误")
                    ));
                }
                if worker
                    .child
                    .try_wait()
                    .map_err(|e| e.to_string())?
                    .is_some()
                {
                    let error =
                        fs::read_to_string(directory.join("worker.log")).unwrap_or_default();
                    return Err(format!(
                        "语音进程退出：{}",
                        error.chars().take(2000).collect::<String>()
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        })();
        self.loaded.store(result.is_ok(), Ordering::Release);
        if result.is_err() {
            *guard = None;
        }
        result
    }
}

pub fn runtime_root(data: &Path) -> PathBuf {
    data.join("learning-runtime")
}
pub fn ready(root: &Path) -> bool {
    [
        "python/python.exe",
        "kokoro/model.int8.onnx",
        "kokoro/voices.bin",
        "kokoro/tokens.txt",
        "kokoro/lexicon-us-en.txt",
        "python/Lib/site-packages/sherpa_onnx/lib/onnxruntime.dll",
    ]
    .iter()
    .all(|p| root.join(p).is_file())
}
pub fn validate(text: &str, speed: f32) -> Result<(), String> {
    if text.trim().is_empty()
        || text.chars().count() > 1500
        || !text.chars().any(|c| c.is_ascii_alphabetic())
        || !(0.6..=1.4).contains(&speed)
    {
        return Err("请输入 1～1500 字的英文，语速范围 0.6～1.4".into());
    }
    Ok(())
}
impl SpeechEngine {
    pub fn synthesize(
        &self,
        root: &Path,
        text: &str,
        speed: f32,
        voice: u32,
        cancel: &AtomicBool,
    ) -> Result<PathBuf, String> {
        validate(text, speed)?;
        if cancel.load(Ordering::Relaxed) {
            return Err("已停止朗读".into());
        }
        if !ready(root) {
            return Err("请先在语音设置中下载 Kokoro 语音模型".into());
        }
        let cache = root.join("audio-cache");
        fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
        let key = format!(
            "{:x}",
            Sha256::digest(format!(
                "kokoro-v1.1-sherpa-1.13.3:{voice}:{speed:.2}:{}:{}",
                text.trim(),
                include_str!("../scripts/kokoro-worker.py")
            ))
        );
        let path = cache.join(format!("{key}.wav"));
        if path.is_file() && duration(&path).is_ok() {
            return Ok(path);
        }
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let output = dir.path().join("speech.wav");
        self.request(
            root,
            serde_json::json!({"text":text.trim(), "voice":voice, "speed":speed, "output":output}),
            cancel,
        )?;
        duration(&output)?;
        // Only our own .wav cache entries are eligible for eviction.
        let mut entries = fs::read_dir(&cache)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "wav"))
            .collect::<Vec<_>>();
        entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
        for e in entries.iter().take(entries.len().saturating_sub(99)) {
            let _ = fs::remove_file(e.path());
        }
        fs::copy(&output, &path).map_err(|e| e.to_string())?;
        Ok(path)
    }
}
fn duration(path: &Path) -> Result<Duration, String> {
    use std::io::Read;
    let mut header = [0u8; 44];
    fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut header))
        .map_err(|e| e.to_string())?;
    if &header[..4] != b"RIFF"
        || &header[8..12] != b"WAVE"
        || &header[12..16] != b"fmt "
        || &header[36..40] != b"data"
        || u32::from_le_bytes(header[16..20].try_into().unwrap()) != 16
        || u16::from_le_bytes(header[20..22].try_into().unwrap()) != 1
    {
        return Err("无效的语音文件".into());
    }
    // The worker emits canonical PCM WAV. Validate its format before trusting duration.
    let channels = u16::from_le_bytes(header[22..24].try_into().unwrap());
    let sample_rate = u32::from_le_bytes(header[24..28].try_into().unwrap());
    let rate = u32::from_le_bytes(header[28..32].try_into().unwrap());
    let alignment = u16::from_le_bytes(header[32..34].try_into().unwrap());
    let bits = u16::from_le_bytes(header[34..36].try_into().unwrap());
    let bytes = u32::from_le_bytes(header[40..44].try_into().unwrap());
    let length = fs::metadata(path).map_err(|e| e.to_string())?.len();
    let riff_bytes = u32::from_le_bytes(header[4..8].try_into().unwrap());
    if channels == 0
        || !matches!(bits, 8 | 16 | 24 | 32)
        || sample_rate == 0
        || u32::from(alignment) != u32::from(channels) * u32::from(bits / 8)
        || alignment == 0
        || !bytes.is_multiple_of(u32::from(alignment))
        || sample_rate.checked_mul(u32::from(alignment)) != Some(rate)
        || rate == 0
        || bytes == 0
        || u64::from(riff_bytes) < u64::from(bytes) + 36
        || length < u64::from(riff_bytes) + 8
    {
        return Err("模型生成了空语音".into());
    }
    Ok(Duration::from_secs_f64(f64::from(bytes) / f64::from(rate)))
}
pub fn play(path: &Path, cancel: &AtomicBool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "winmm")]
        unsafe extern "system" {
            fn PlaySoundW(sound: *const u16, module: *mut std::ffi::c_void, flags: u32) -> i32;
        }
        let time = duration(path)?;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if cancel.load(Ordering::Relaxed) {
            return Err("已停止朗读".into());
        }
        if unsafe { PlaySoundW(wide.as_ptr(), std::ptr::null_mut(), 0x00020003) } == 0 {
            return Err("无法播放语音，请检查音频设备".into());
        }
        let started = std::time::Instant::now();
        while started.elapsed() < time {
            if cancel.load(Ordering::Relaxed) {
                unsafe {
                    PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0);
                }
                return Err("已停止朗读".into());
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (path, cancel);
        Err("当前播放仅支持 Windows".into())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn speech_speed_bounds_and_cancellation_do_not_require_model_files() {
        for speed in [0.6, 1.0, 1.4] {
            assert!(validate("hello", speed).is_ok());
        }
        for speed in [0.59, 1.41, f32::NAN, f32::INFINITY] {
            assert!(validate("hello", speed).is_err());
        }
        let dir = tempfile::tempdir().unwrap();
        let engine = SpeechEngine::default();
        assert!(
            engine
                .synthesize(dir.path(), "hello", 1., 0, &AtomicBool::new(true))
                .unwrap_err()
                .contains("停止")
        );
        assert!(!engine.is_loaded());
        assert!(!dir.path().join("audio-cache").exists());
        engine.stop();
        engine.stop();
        assert!(!engine.is_loaded());
    }

    #[test]
    fn invalid_or_truncated_audio_is_not_accepted_as_a_cache_hit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("voice.wav");
        for bytes in [vec![], vec![0; 44], b"RIFFshort".to_vec()] {
            fs::write(&path, bytes).unwrap();
            assert!(duration(&path).is_err());
        }
        let mut wav = vec![0; 48];
        wav[..4].copy_from_slice(b"RIFF");
        wav[4..8].copy_from_slice(&40_u32.to_le_bytes());
        wav[8..12].copy_from_slice(b"WAVE");
        wav[12..16].copy_from_slice(b"fmt ");
        wav[16..20].copy_from_slice(&16_u32.to_le_bytes());
        wav[20..22].copy_from_slice(&1_u16.to_le_bytes());
        wav[22..24].copy_from_slice(&1_u16.to_le_bytes());
        wav[24..28].copy_from_slice(&2_u32.to_le_bytes());
        wav[32..34].copy_from_slice(&2_u16.to_le_bytes());
        wav[34..36].copy_from_slice(&16_u16.to_le_bytes());
        wav[36..40].copy_from_slice(b"data");
        wav[28..32].copy_from_slice(&4_u32.to_le_bytes());
        wav[40..44].copy_from_slice(&4_u32.to_le_bytes());
        fs::write(&path, &wav).unwrap();
        assert_eq!(duration(&path).unwrap(), Duration::from_secs(1));
        for offset in [4, 12, 16, 20, 22, 24, 28, 32, 34, 40] {
            let mut invalid = wav.clone();
            invalid[offset] ^= 0xff;
            fs::write(&path, invalid).unwrap();
            assert!(duration(&path).is_err(), "offset {offset}");
        }
        wav.truncate(46);
        fs::write(&path, &wav).unwrap();
        assert!(duration(&path).is_err());
    }
    #[test]
    fn speech_rejects_empty_oversized_and_non_english_input() {
        assert!(validate("", 1.).is_err());
        assert!(validate("中文", 1.).is_err());
        assert!(validate(&"a".repeat(1501), 1.).is_err());
        assert!(validate("English", 0.).is_err());
        assert!(validate("How are you?", 0.75).is_ok());
    }
    #[test]
    #[ignore = "requires local Kokoro; measures cold startup, reuse, cancellation and restart without playback"]
    fn live_resident_worker_reuse_cancel_and_restart() {
        let root = PathBuf::from(std::env::var_os("PERSONAL_DESK_LEARNING_RUNTIME").unwrap());
        let engine = SpeechEngine::default();
        let cancel = AtomicBool::new(false);
        let started = Instant::now();
        engine.warmup(&root, 0, &cancel).unwrap();
        println!("kokoro cold warmup: {:?}", started.elapsed());
        let pid = engine.worker.lock().unwrap().as_ref().unwrap().child.id();
        let temporary = tempfile::tempdir().unwrap();
        for (index, (text, voice)) in [
            ("architecture", 0),
            ("I am learning English every day.", 0),
            ("Good morning.", 2),
        ]
        .into_iter()
        .enumerate()
        {
            let output = temporary.path().join(format!("{index}.wav"));
            let started = Instant::now();
            engine
                .request(
                    &root,
                    serde_json::json!({"text":text,"voice":voice,"speed":1.0,"output":output}),
                    &cancel,
                )
                .unwrap();
            assert!(duration(&output).unwrap() > Duration::from_millis(100));
            assert_eq!(
                pid,
                engine.worker.lock().unwrap().as_ref().unwrap().child.id()
            );
            println!(
                "kokoro voice {voice}: {text:?}, synthesis: {:?}",
                started.elapsed()
            );
        }
        // Cancel a real in-flight request and ensure its process is reclaimed.
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(150));
                cancel.store(true, Ordering::Release);
            });
            let output = temporary.path().join("cancelled.wav");
            assert!(engine.request(&root, serde_json::json!({"text":"This sentence will be cancelled. ".repeat(35),"voice":2,"speed":1.0,"output":output}), &cancel).is_err());
        });
        assert!(!engine.is_loaded());
        assert!(engine.worker.lock().unwrap().is_none());
        cancel.store(false, Ordering::Release);
        engine.warmup(&root, 0, &cancel).unwrap();
        assert!(engine.is_loaded());
        engine.stop();
        assert!(!engine.is_loaded());
        assert!(engine.worker.lock().unwrap().is_none());
    }
    #[test]
    #[ignore = "requires downloaded Kokoro; set PERSONAL_DESK_LEARNING_RUNTIME; plays a short word"]
    fn live_word_sentence_both_accents_cache_and_playback() {
        let root = PathBuf::from(std::env::var_os("PERSONAL_DESK_LEARNING_RUNTIME").unwrap());
        assert!(ready(&root));
        let engine = SpeechEngine::default();
        for (text, speed, voice) in [
            ("architecture", 1., 0),
            ("I am learning English every day.", 1., 0),
            ("I am learning English every day.", 0.75, 2),
        ] {
            let cancel = AtomicBool::new(false);
            let path = engine
                .synthesize(&root, text, speed, voice, &cancel)
                .unwrap();
            let length = duration(&path).unwrap();
            assert!(length > Duration::from_millis(200));
            let modified = fs::metadata(&path).unwrap().modified().unwrap();
            assert_eq!(
                path,
                engine
                    .synthesize(&root, text, speed, voice, &cancel)
                    .unwrap()
            );
            assert_eq!(modified, fs::metadata(&path).unwrap().modified().unwrap());
            if text == "architecture" {
                play(&path, &cancel).unwrap();
            }
            println!("voice={voice}, speed={speed}, duration={length:?}");
        }
    }
}
