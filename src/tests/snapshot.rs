use std::{
    fs::File,
    io::IsTerminal,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose, Engine};
use dssim::Dssim;
use eframe::epaint::Vec2;
use egui_skia::draw_onto_surface;
use image::{DynamicImage, ImageOutputFormat, RgbImage};
use log::info;
use num::BigInt;
use project_root::get_project_root;
use skia_safe::EncodedImageFormat;
use spade_common::num_ext::InfallibleToBigInt;
use test_log::test;

use crate::{
    clock_highlighting::ClockHighlightType,
    wave_container::{FieldRef, ModuleRef, SignalRef},
    Message, StartupParams, State, WaveSource,
};

fn print_image(img: &DynamicImage) {
    if std::io::stdout().is_terminal() {
        let mut bytes = vec![];
        img.write_to(
            &mut std::io::Cursor::new(&mut bytes),
            ImageOutputFormat::Png,
        )
        .unwrap();
        let b64 = general_purpose::STANDARD.encode(&bytes);
        println!(
            "\x1b]1337;File=size={size};width=auto;height=auto;inline=1:{b64}\x1b]\x1b[1E",
            size = bytes.len()
        )
    }
}

fn to_byte(i: f32) -> u8 {
    if i <= 0.0 {
        0
    } else if i >= 255.0 / 256.0 {
        255
    } else {
        (i * 256.0) as u8
    }
}

fn render_and_compare(filename: &Path, state: impl Fn() -> State) {
    info!("test up and running");

    // https://tokio.rs/tokio/topics/bridging
    // We want to run the gui in the main thread, but some long running tasks like
    // laoading VCDs should be done asynchronously. We can't just use std::thread to
    // do that due to wasm support, so we'll start a tokio runtime
    let runtime = tokio::runtime::Builder::new_current_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();

    let _enter = runtime.enter();

    std::thread::spawn(move || {
        runtime.block_on(async {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
            }
        })
    });

    let mut state = state();
    state.show_wave_source = false;

    let size = Vec2::new(1280., 720.);
    let size_i = (size.x as i32, size.y as i32);

    let mut surface =
        skia_safe::surfaces::raster_n32_premul(size_i).expect("Failed to create surface");
    surface.canvas().clear(skia_safe::Color::BLACK);

    draw_onto_surface(
        &mut surface,
        |ctx| {
            ctx.memory_mut(|mem| mem.options.tessellation_options.feathering = false);
            ctx.set_visuals(state.get_visuals());

            state.draw(ctx, Some(size));
        },
        Some(egui_skia::RasterizeOptions {
            frames_before_screenshot: 2,
            ..Default::default()
        }),
    );

    // NOTE: The warning suggests a method which rust-analyzer doesn't find
    #[allow(deprecated)]
    let data = surface
        .image_snapshot()
        .encode_to_data(EncodedImageFormat::PNG)
        .expect("Failed to encode image");
    let new = image::load_from_memory(&data).expect("Failed to decode png with image crate");

    let root = get_project_root().expect("Failed to get root");

    let previous_image_file = root.join("snapshots").join(filename).with_extension("png");

    let (write_new_file, diff) = if previous_image_file.exists() {
        let mut comparator = Dssim::new();
        comparator.set_save_ssim_maps(1);
        let prev = dssim::load_image(&comparator, &previous_image_file).expect(&format!(
            "Failed to load previous image from {previous_image_file:?}"
        ));
        let new = comparator
            .create_image_rgb(
                &new.to_rgb8()
                    .pixels()
                    .map(|p| rgb::RGB {
                        r: p[0],
                        g: p[1],
                        b: p[2],
                    })
                    .collect::<Vec<_>>(),
                new.width() as usize,
                new.height() as usize,
            )
            .expect("Failed to create dssim image from new");

        // comparator.create_image_rgb(&prev_imgref.as_ref(), width, height);
        let (score, map) = comparator.compare(&prev, &new);
        (score != 0., Some((score, map)))
    } else {
        (true, None)
    };

    let new_file = root
        .join("snapshots")
        .join(filename)
        .with_extension("new.png");

    if new_file.exists() {
        std::fs::remove_file(&new_file).expect("Failed to remove existing snapshot file");
    }
    if write_new_file {
        std::fs::create_dir_all("snapshots").expect("Failed to create snapshots dir");
        new.write_to(
            &mut File::create(&new_file).expect(&format!("Failed to create {new_file:?}")),
            ImageOutputFormat::Png,
        )
        .expect(&format!("Failed to write new image to {new_file:?}"));
    }

    match (write_new_file, diff) {
        (true, Some((score, maps))) => {
            let map = maps.first().unwrap();
            let avgssim = map.ssim as f32;
            let out: Vec<_> = map
                .map
                .pixels()
                .flat_map(|ssim| {
                    let max = 1_f32 - ssim;
                    let maxsq = max * max;
                    [
                        to_byte(maxsq * 16.0),
                        to_byte(max * 3.0),
                        to_byte(max / ((1_f32 - avgssim) * 4_f32)),
                    ]
                })
                .collect();

            let diff_img = RgbImage::from_vec(map.map.width() as u32, map.map.height() as u32, out)
                .expect("Failed to create Image::image from diff");
            let diff_file = root
                .join("snapshots")
                .join(filename)
                .with_extension("diff.png");

            diff_img
                .save(&diff_file)
                .expect(&format!("Failed to save diff file to {diff_file:?}"));

            println!("Previous: {previous_image_file:?}");
            // The Dssim image is super annoying to work with, so I'll just reload the image
            print_image(&image::open(&previous_image_file).expect("Failed to load prev image"));
            println!("New: {new_file:?}");
            print_image(&new);

            println!("Diff: {diff_file:?}");
            print_image(&DynamicImage::ImageRgb8(diff_img));

            assert!(
                false,
                "Snapshot diff. Score: {score}\n\told: {previous_image_file:?}\n\tnew: {new_file:?}"
            )
        }
        (true, None) => {
            print_image(&new);
            assert!(false, "New snapshot image (saved to {new_file:?})")
        }
        (false, _) => {}
    }
}

macro_rules! snapshot_ui {
    ($name:ident, $state:expr) => {
        #[test]
        fn $name() {
            render_and_compare(&PathBuf::from(stringify!($name)), $state)
        }
    };
}

macro_rules! snapshot_empty_state_with_msgs {
    ($name:ident, $msgs:expr) => {
        snapshot_ui! {$name, || {
            let mut state = State::new().unwrap().with_params(StartupParams::empty());
            for msg in $msgs {
                state.update(msg);
            }
            state
        }}
    };
}

macro_rules! snapshot_ui_with_file_and_msgs {
    ($name:ident, $file:expr, state_mods: $initial_state_mods:expr, $msgs:expr) => {
        snapshot_ui_with_file_spade_and_msgs!($name, $file, None, None, $initial_state_mods, $msgs);
    };
    ($name:ident, $file:expr, $msgs:expr) => {
        snapshot_ui_with_file_spade_and_msgs!($name, $file, None, None, (|_state| {}), $msgs);
    };
}

macro_rules! snapshot_ui_with_file_spade_and_msgs {
    ($name:ident, $file:expr, $spade_top:expr, $spade_state:expr, $msgs:expr) => {
        snapshot_ui_with_file_spade_and_msgs!(
            $name,
            $file,
            $spade_top,
            $spade_state,
            (|_state| {}),
            $msgs
        );
    };
    ($name:ident, $file:expr, $spade_top:expr, $spade_state:expr, $initial_state_mod:expr, $msgs:expr) => {
        snapshot_ui!($name, || {
            let spade_top = $spade_top;
            let mut state = State::new().unwrap().with_params(StartupParams {
                waves: Some(WaveSource::File(
                    get_project_root().unwrap().join($file).try_into().unwrap(),
                )),
                spade_top: spade_top.clone(),
                spade_state: $spade_state,
                startup_commands: vec![],
            });

            $initial_state_mod(&mut state);

            let load_start = std::time::Instant::now();

            loop {
                state.handle_async_messages();
                let spade_loaded = if spade_top.is_some() {
                    state
                        .sys
                        .translators
                        .all_translator_names()
                        .iter()
                        .any(|n| *n == "spade")
                } else {
                    true
                };

                if state.waves.is_some() && spade_loaded {
                    break;
                }

                if load_start.elapsed().as_secs() > 100 {
                    panic!("Timeout")
                }
            }
            state.update(Message::ToggleMenu);
            state.update(Message::ToggleSidePanel);

            for msg in $msgs {
                state.update(msg)
            }

            state
        });
    };
}

snapshot_ui! {startup_screen_looks_fine, || {
    State::new().unwrap().with_params(StartupParams::empty())
}}

snapshot_ui!(menu_can_be_hidden, || {
    let mut state = State::new().unwrap().with_params(StartupParams::empty());
    let msgs = [Message::ToggleMenu];
    for message in msgs.into_iter() {
        state.update(message);
    }
    state
});

snapshot_ui!(side_panel_can_be_hidden, || {
    let mut state = State::new().unwrap().with_params(StartupParams::empty());
    let msgs = [Message::ToggleSidePanel];
    for message in msgs.into_iter() {
        state.update(message);
    }
    state
});

snapshot_ui! {example_vcd_renders, || {
    let mut state = State::new().unwrap().with_params(StartupParams {
        waves: Some(WaveSource::File(get_project_root().unwrap().join("examples/counter.vcd").try_into().unwrap())),
        spade_top: None,
        spade_state: None,
        startup_commands: vec![]
    });

    loop {
        state.handle_async_messages();
        if state.waves.is_some() {
            break;
        }
    }

    state.update(Message::ToggleMenu);
    state.update(Message::ToggleSidePanel);
    state.update(Message::AddModule(ModuleRef::from_strs(&["tb"])));
    state.update(Message::AddModule(ModuleRef::from_strs(&["tb", "dut"])));

    state
}}

snapshot_empty_state_with_msgs! {
    dialogs_work,
    [
        Message::SetUrlEntryVisible(true),
        Message::SetKeyHelpVisible(true),
        Message::SetGestureHelpVisible(true),
    ]
}
snapshot_empty_state_with_msgs! {
    quick_start_works,
    [
        Message::SetQuickStartVisible(true),
    ]
}

snapshot_ui_with_file_and_msgs! {top_level_signals_have_no_aliasing, "examples/picorv32.vcd", [
    Message::AddModule(ModuleRef::from_strs(&["testbench"]))
]}

snapshot_ui! {resizing_the_canvas_redraws, || {
    let mut state = State::new().unwrap().with_params(StartupParams {
        waves: Some(WaveSource::File(get_project_root().unwrap().join("examples/counter.vcd").try_into().unwrap())),
        spade_top: None,
        spade_state: None,
        startup_commands: vec![]
    });

    loop {
        state.handle_async_messages();
        if state.waves.is_some() {
            break;
        }
    }

    state.update(Message::AddModule(ModuleRef::from_strs(&["tb"])));
    state.update(Message::CursorSet(100u32.to_bigint()));

    // Render the UI once with the sidebar shown
    let size = Vec2::new(1280., 720.);
    let size_i = (size.x as i32, size.y as i32);
    let mut surface = skia_safe::surfaces::raster_n32_premul(size_i).expect("Failed to create surface");
    surface.canvas().clear(skia_safe::Color::BLACK);

    draw_onto_surface(
        &mut surface,
        |ctx| {
            ctx.memory_mut(|mem| mem.options.tessellation_options.feathering = false);
            ctx.set_visuals(state.get_visuals());

            state.draw(ctx, Some(size));
        },
        None,
    );

    state.update(Message::ToggleSidePanel);

    state
}}

snapshot_ui_with_file_and_msgs! {clock_pulses_render_line, "examples/counter.vcd", [
    Message::AddModule(ModuleRef::from_strs(&["tb"])),
    Message::SignalFormatChange(FieldRef::from_strs(&["tb", "clk"], &[]), String::from("Clock")),
    Message::SetClockHighlightType(ClockHighlightType::Line),
]}

snapshot_ui_with_file_and_msgs! {clock_pulses_render_cycle, "examples/counter.vcd", [
    Message::AddModule(ModuleRef::from_strs(&["tb"])),
    Message::SignalFormatChange(FieldRef::from_strs(&["tb", "clk"], &[]), String::from("Clock")),
    Message::SetClockHighlightType(ClockHighlightType::Cycle),
]}

snapshot_ui_with_file_and_msgs! {clock_pulses_render_none, "examples/counter.vcd", [
    Message::AddModule(ModuleRef::from_strs(&["tb"])),
    Message::SignalFormatChange(FieldRef::from_strs(&["tb", "clk"], &[]), String::from("Clock")),
    Message::SetClockHighlightType(ClockHighlightType::None),
]}

snapshot_ui_with_file_and_msgs! {vertical_scrolling_works, "examples/picorv32.vcd", [
    Message::AddModule(ModuleRef::from_strs(&["testbench", "top", "mem"])),
    Message::VerticalScroll(crate::MoveDir::Down, 5),
    Message::VerticalScroll(crate::MoveDir::Up, 2),
]}

snapshot_ui_with_file_and_msgs! {vcd_with_empty_scope_loads, "examples/verilator_empty_scope.vcd", [
    Message::AddModule(ModuleRef::from_strs(&["top_test"])),
]}

snapshot_ui_with_file_spade_and_msgs! {
    spade_translation_works,
    "examples/spade.vcd",
    Some("proj::pipeline_ready_valid::ready_valid_pipeline".to_string()),
    Some("examples/spade_state.ron".into()),
    [
    Message::AddModule(ModuleRef::from_strs(&[
        "proj::pipeline_ready_valid::ready_valid_pipeline"
    ])),
    ]
}

snapshot_ui_with_file_and_msgs! {divider_works, "examples/counter.vcd", [
    Message::AddModule(ModuleRef::from_strs(&["tb"])),
    Message::AddDivider("Divider".to_string()),
    Message::AddModule(ModuleRef::from_strs(&["tb"])),
    Message::ItemBackgroundColorChange(Some(4), Some("Blue".to_string())),
    Message::ItemColorChange(Some(4), Some("Green".to_string()))
]}

snapshot_ui_with_file_and_msgs! {cursors_work, "examples/counter.vcd", [
    Message::AddModule(ModuleRef::from_strs(&["tb"])),
    Message::CursorSet(BigInt::from(600)),
    Message::SetCursorPosition(2),
    Message::ItemColorChange(Some(4), Some("Blue".to_string())),
    Message::CursorSet(BigInt::from(200)),
    Message::SetCursorPosition(1),
    Message::ItemColorChange(Some(5), Some("Green".to_string())),
    Message::CursorSet(BigInt::from(500)),
]}

snapshot_ui_with_file_and_msgs! {
    startup_commands_work,
    "examples/counter.vcd",
    state_mods: (|state: &mut State| {
        state.sys.startup_commands = vec!["module_add tb".to_string()];
    }),
    []
}

snapshot_ui_with_file_and_msgs! {signals_are_added_at_focus, "examples/counter.vcd", [
    Message::AddModule(ModuleRef::from_strs(&["tb"])),
    Message::FocusItem(1),
    Message::AddSignal(SignalRef::from_hierarchy_string("tb.dut.counter"))
]}

snapshot_ui_with_file_and_msgs! {dividers_are_added_at_focus, "examples/counter.vcd", [
    Message::AddModule(ModuleRef::from_strs(&["tb"])),
    Message::FocusItem(1),
    Message::AddDivider(String::from("Test"))
]}

snapshot_ui_with_file_and_msgs! {dividers_are_appended_without_focus, "examples/counter.vcd", [
    Message::AddModule(ModuleRef::from_strs(&["tb"])),
    Message::AddDivider(String::from("Test"))
]}
