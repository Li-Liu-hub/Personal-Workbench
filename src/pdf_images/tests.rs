use super::*;

pub(crate) fn fixture(path: &Path) {
    fixture_pages(path, &[(144, 72), (72, 144), (108, 108)]);
}

fn fixture_pages(path: &Path, sizes: &[(u32, u32)]) {
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R 5 0 R 7 0 R] /Count 3 >>".to_owned(),
    ];
    for (index, (w, h)) in sizes.iter().copied().enumerate() {
        let content = format!("1 1 1 rg 0 0 {w} {h} re f 1 0 0 rg 0 0 36 36 re f");
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Resources << >> /Contents {} 0 R >>", 4 + index * 2));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ));
    }
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    std::fs::write(path, pdf).unwrap();
}

fn options(dir: &Path) -> Options {
    let input = dir.join("中文 测试.pdf");
    fixture(&input);
    Options {
        input,
        output: dir.into(),
        format: Format::Png,
        dpi: 96,
        quality: 90,
        pages: String::new(),
    }
}

#[test]
fn ranges_validate_and_deduplicate() {
    assert_eq!(page_numbers("3,1-2,2，3", 3).unwrap(), vec![1, 2, 3]);
    assert_eq!(page_numbers("", 3).unwrap(), vec![1, 2, 3]);
    for text in ["0", "4", "2-1", "1,", "1-2-3", "-1", "a", "1-999999999"] {
        assert!(page_numbers(text, 3).is_err(), "{text}");
    }
    assert!(page_numbers("", 0).is_err());
}

#[test]
#[ignore = "requires PERSONAL_DESK_PDF_FIXTURE and PERSONAL_DESK_PDF_OUTPUT for visual acceptance"]
fn render_visual_acceptance_document() {
    let input =
        PathBuf::from(std::env::var_os("PERSONAL_DESK_PDF_FIXTURE").expect("PDF fixture path"));
    let output = PathBuf::from(std::env::var_os("PERSONAL_DESK_PDF_OUTPUT").expect("output path"));
    std::fs::create_dir_all(&output).unwrap();
    for format in Format::ALL {
        let opts = Options {
            input: input.clone(),
            output: output.clone(),
            format,
            dpi: 150,
            quality: 92,
            pages: String::new(),
        };
        let result = convert(&opts, &AtomicBool::new(false), |_, _, _| {}).unwrap();
        assert_eq!(result.pages, 3);
        println!("{}: {}", format.label(), result.folder.display());
    }
    let opts = Options {
        input: input.with_file_name("encrypted.pdf"),
        output,
        format: Format::Png,
        dpi: 150,
        quality: 92,
        pages: String::new(),
    };
    assert!(
        convert(&opts, &AtomicBool::new(false), |_, _, _| {})
            .unwrap_err()
            .contains("密码")
    );
}

#[test]
fn native_pdf_converts_all_formats_with_correct_page_dimensions_and_pixels() {
    use image::GenericImageView;
    let dir = tempfile::tempdir().unwrap();
    let mut opts = options(dir.path());
    let original = std::fs::read(&opts.input).unwrap();
    let mut folders = BTreeSet::new();
    for format in Format::ALL {
        opts.format = format;
        let mut events = Vec::new();
        let result = convert(&opts, &AtomicBool::new(false), |a, b, c| {
            events.push((a, b, c))
        })
        .unwrap();
        assert_eq!(result.pages, 3);
        assert!(folders.insert(result.folder.clone()));
        assert_eq!(events.last(), Some(&(3, 3, 3)));
        for (page, dimensions) in [(1, (192, 96)), (2, (96, 192)), (3, (144, 144))] {
            let bytes = std::fs::read(
                result
                    .folder
                    .join(format!("中文 测试_{page:04}.{}", format.extension())),
            )
            .unwrap();
            let expected = match format {
                Format::Png => image::ImageFormat::Png,
                Format::Jpg => image::ImageFormat::Jpeg,
                Format::WebP => image::ImageFormat::WebP,
                Format::Bmp => image::ImageFormat::Bmp,
            };
            assert_eq!(image::guess_format(&bytes).unwrap(), expected);
            let picture = image::load_from_memory(&bytes).unwrap();
            assert_eq!(picture.dimensions(), dimensions);
            let white = picture.get_pixel(dimensions.0 - 10, 10);
            assert!(white[0] > 245 && white[1] > 245 && white[2] > 245);
            let red = picture.get_pixel(10, dimensions.1 - 10);
            assert!(red[0] > 240 && red[1] < 15 && red[2] < 15);
        }
    }
    assert_eq!(std::fs::read(&opts.input).unwrap(), original);
}

#[test]
fn selection_dpi_unique_batches_and_failure_cleanup_preserve_existing_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut opts = options(dir.path());
    let sentinel = dir.path().join("已有图片.png");
    std::fs::write(&sentinel, b"keep").unwrap();
    opts.pages = "3,1,3".into();
    opts.dpi = 150;
    let first = convert(&opts, &AtomicBool::new(false), |_, _, _| {}).unwrap();
    assert_eq!(first.pages, 2);
    assert_eq!(std::fs::read_dir(&first.folder).unwrap().count(), 2);
    assert_eq!(
        image::open(first.folder.join("中文 测试_0001.png"))
            .unwrap()
            .width(),
        300
    );
    let second = convert(&opts, &AtomicBool::new(false), |_, _, _| {}).unwrap();
    assert_ne!(first.folder, second.folder);
    let before = std::fs::read_dir(dir.path()).unwrap().count();
    opts.pages.clear();
    let cancel = AtomicBool::new(false);
    assert!(
        convert(&opts, &cancel, |done, _, _| {
            if done == 1 {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap_err()
        .contains("取消")
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), before);
    fixture_pages(&opts.input, &[(144, 72), (100_000, 100_000), (108, 108)]);
    let mut completed = 0;
    let error = convert(&opts, &AtomicBool::new(false), |done, _, _| {
        completed = done
    })
    .unwrap_err();
    assert!(error.contains("过大"), "{error}");
    assert_eq!(completed, 1);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), before);
    opts.pages = "100".into();
    assert!(convert(&opts, &AtomicBool::new(false), |_, _, _| {}).is_err());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), before);
    std::fs::write(&opts.input, b"broken PDF").unwrap();
    assert!(convert(&opts, &AtomicBool::new(false), |_, _, _| {}).is_err());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), before);
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep");
}
