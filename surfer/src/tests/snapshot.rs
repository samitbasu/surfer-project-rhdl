use std::{
    fs::File,
    io::IsTerminal,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose, Engine};
use dssim::Dssim;
use eframe::epaint::Vec2;
use egui_skia::draw_onto_surface;
use image::{DynamicImage, ImageFormat, RgbImage};
use log::info;
use num::BigInt;
use project_root::get_project_root;
use skia_safe::EncodedImageFormat;
use test_log::test;

use crate::{
    clock_highlighting::ClockHighlightType,
    config::HierarchyStyle,
    displayed_item::DisplayedFieldRef,
    displayed_item::DisplayedItemIndex,
    displayed_item::DisplayedItemRef,
    setup_custom_font,
    variable_name_filter::VariableNameFilterType,
    wave_container::{ScopeRef, VariableRef},
    wave_source::LoadOptions,
    Message, MoveDir, StartupParams, State, WaveSource,
};

fn print_image(img: &DynamicImage) {
    if std::io::stdout().is_terminal() {
        let mut bytes = vec![];
        img.write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
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

pub(crate) fn render_and_compare(filename: &Path, state: impl Fn() -> State) {
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
    state.config.layout.show_statusbar = false;

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
            setup_custom_font(ctx);
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
        let prev = dssim::load_image(&comparator, &previous_image_file).unwrap_or_else(|_| {
            panic!("Failed to load previous image from {previous_image_file:?}")
        });
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
        let (score, map) = comparator.compare(&prev, new);
        (score >= 1e-6, Some((score, map)))
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
            &mut File::create(&new_file)
                .unwrap_or_else(|_| panic!("Failed to create {new_file:?}")),
            ImageFormat::Png,
        )
        .unwrap_or_else(|_| panic!("Failed to write new image to {new_file:?}"));
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
                .unwrap_or_else(|_| panic!("Failed to save diff file to {diff_file:?}"));

            println!("Previous: {previous_image_file:?}");
            // The Dssim image is super annoying to work with, so I'll just reload the image
            print_image(&image::open(&previous_image_file).expect("Failed to load prev image"));
            println!("New: {new_file:?}");
            print_image(&new);

            println!("Diff: {diff_file:?}");
            print_image(&DynamicImage::ImageRgb8(diff_img));

            panic!(
                "Snapshot diff. Score: {score}\n\told: {previous_image_file:?}\n\tnew: {new_file:?}"
            )
        }
        (true, None) => {
            print_image(&new);
            panic!("New snapshot image (saved to {new_file:?})")
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
            let mut state = State::new_default_config().unwrap().with_params(StartupParams::empty());
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
            let mut state = State::new_default_config()
                .unwrap()
                .with_params(StartupParams {
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
                state.handle_batch_commands();
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

                if state.waves_fully_loaded() && spade_loaded {
                    break;
                }

                if load_start.elapsed().as_secs() > 100 {
                    panic!("Timeout")
                }
            }
            state.add_startup_message(Message::ToggleMenu);
            state.add_startup_message(Message::ToggleSidePanel);
            state.add_startup_message(Message::ToggleToolbar);
            state.add_startup_message(Message::ToggleOverview);
            state.add_startup_messages($msgs);

            // make sure all the signals added by the proceeding messages are properly loaded
            wait_for_waves_fully_loaded(&mut state, 10);

            state
        });
    };
}

snapshot_ui! {startup_screen_looks_fine, || {
    State::new_default_config().unwrap().with_params(StartupParams::empty())
}}

snapshot_ui!(menu_can_be_hidden, || {
    let mut state = State::new_default_config()
        .unwrap()
        .with_params(StartupParams::empty());
    let msgs = [Message::ToggleMenu];
    for message in msgs.into_iter() {
        state.update(message);
    }
    state
});

snapshot_ui!(side_panel_can_be_hidden, || {
    let mut state = State::new_default_config()
        .unwrap()
        .with_params(StartupParams::empty());
    let msgs = [Message::ToggleSidePanel];
    for message in msgs.into_iter() {
        state.update(message);
    }
    state
});

snapshot_ui!(toolbar_can_be_hidden, || {
    let mut state = State::new_default_config()
        .unwrap()
        .with_params(StartupParams::empty());
    let msgs = [Message::ToggleToolbar];
    for message in msgs.into_iter() {
        state.update(message);
    }
    state
});

snapshot_ui!(overview_can_be_hidden, || {
    let mut state = State::new_default_config()
        .unwrap()
        .with_params(StartupParams {
            waves: Some(WaveSource::File(
                get_project_root()
                    .unwrap()
                    .join("examples/counter.vcd")
                    .try_into()
                    .unwrap(),
            )),
            spade_top: None,
            spade_state: None,
            startup_commands: vec![],
        });

    loop {
        state.handle_async_messages();
        state.handle_batch_commands();
        if state.waves_fully_loaded() {
            break;
        }
    }
    state.update(Message::AddVariables(vec![
        VariableRef::from_hierarchy_string("tb.dut.counter"),
    ]));
    state.update(Message::CursorSet(BigInt::from(10)));
    state.update(Message::ToggleOverview);
    // make sure all the signals added by the proceeding messages are properly loaded
    wait_for_waves_fully_loaded(&mut state, 10);
    state
});

snapshot_ui!(statusbar_can_be_hidden, || {
    let mut state = State::new_default_config()
        .unwrap()
        .with_params(StartupParams {
            waves: Some(WaveSource::File(
                get_project_root()
                    .unwrap()
                    .join("examples/counter.vcd")
                    .try_into()
                    .unwrap(),
            )),
            spade_top: None,
            spade_state: None,
            startup_commands: vec![],
        });

    loop {
        state.handle_async_messages();
        state.handle_batch_commands();
        if state.waves_fully_loaded() {
            break;
        }
    }
    state.update(Message::AddVariables(vec![
        VariableRef::from_hierarchy_string("tb.dut.counter"),
    ]));
    state.update(Message::CursorSet(BigInt::from(10)));
    state.update(Message::ToggleStatusbar);
    // make sure all the signals added by the proceeding messages are properly loaded
    wait_for_waves_fully_loaded(&mut state, 10);
    state
});

snapshot_ui! {example_vcd_renders, || {
    let mut state = State::new_default_config().unwrap().with_params(StartupParams {
        waves: Some(WaveSource::File(get_project_root().unwrap().join("examples/counter.vcd").try_into().unwrap())),
        spade_top: None,
        spade_state: None,
        startup_commands: vec![]
    });

    loop {
        state.handle_async_messages();
        state.handle_batch_commands();
        if state.waves_fully_loaded() {
            break;
        }
    }

    state.update(Message::ToggleMenu);
    state.update(Message::ToggleSidePanel);
    state.update(Message::ToggleToolbar);
    state.update(Message::ToggleOverview);
    state.update(Message::AddScope(ScopeRef::from_strs(&["tb"])));
    state.update(Message::AddScope(ScopeRef::from_strs(&["tb", "dut"])));
    // make sure all the signals added by the proceeding messages are properly loaded
    wait_for_waves_fully_loaded(&mut state, 10);
    state
}}

snapshot_empty_state_with_msgs! {
    dialogs_work,
    [
        Message::ToggleMenu,
        Message::ToggleSidePanel,
        Message::ToggleToolbar,
        Message::ToggleOverview,
        Message::SetUrlEntryVisible(true),
        Message::SetKeyHelpVisible(true),
        Message::SetGestureHelpVisible(true),
        Message::SetLicenseVisible(true),
    ]
}
snapshot_empty_state_with_msgs! {
    quick_start_works,
    [
        Message::SetQuickStartVisible(true),
    ]
}

snapshot_ui_with_file_and_msgs! {top_level_signals_have_no_aliasing, "examples/picorv32.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["testbench"]))
]}

snapshot_ui! {resizing_the_canvas_redraws, || {
    let mut state = State::new_default_config().unwrap().with_params(StartupParams {
        waves: Some(WaveSource::File(get_project_root().unwrap().join("examples/counter.vcd").try_into().unwrap())),
        spade_top: None,
        spade_state: None,
        startup_commands: vec![]
    });

    loop {
        state.handle_async_messages();
        state.handle_batch_commands();
        if state.waves_fully_loaded() {
            break;
        }
    }

    state.update(Message::ToggleMenu);
    state.update(Message::ToggleToolbar);
    state.update(Message::ToggleOverview);
    state.update(Message::AddScope(ScopeRef::from_strs(&["tb"])));
    state.update(Message::CursorSet(BigInt::from(100)));
    // make sure all the signals added by the proceeding messages are properly loaded
    wait_for_waves_fully_loaded(&mut state, 10);

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
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::VariableFormatChange(DisplayedFieldRef{item: DisplayedItemRef(2), field: vec![]}, String::from("Clock")),
    Message::SetClockHighlightType(ClockHighlightType::Line),
]}

snapshot_ui_with_file_and_msgs! {clock_pulses_render_cycle, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::VariableFormatChange(DisplayedFieldRef{item: DisplayedItemRef(2), field: vec![]}, String::from("Clock")),
    Message::SetClockHighlightType(ClockHighlightType::Cycle),
]}

snapshot_ui_with_file_and_msgs! {clock_pulses_render_none, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::VariableFormatChange(DisplayedFieldRef{item: DisplayedItemRef(2), field: vec![]}, String::from("Clock")),
    Message::SetClockHighlightType(ClockHighlightType::None),
]}

snapshot_ui_with_file_and_msgs! {vertical_scrolling_works, "examples/picorv32.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["testbench", "top", "mem"])),
    Message::VerticalScroll(crate::MoveDir::Down, 5),
    Message::VerticalScroll(crate::MoveDir::Up, 2),
]}

snapshot_ui_with_file_and_msgs! {vcd_with_empty_scope_loads, "examples/verilator_empty_scope.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["top_test"])),
]}

snapshot_ui_with_file_and_msgs! {fst_with_sv_data_types_loads, "examples/many_sv_datatypes.fst", [
    Message::AddScope(ScopeRef::from_strs(&["TOP", "SVDataTypeWrapper", "bb"])),
]}

snapshot_ui_with_file_and_msgs! {fst_from_vhdl_loads, "examples/vhdl3.fst", [
    Message::AddScope(ScopeRef::from_strs(&["test", "rr"])),
]}

snapshot_ui_with_file_and_msgs! {vcd_from_vhdl_loads, "examples/vhdl3.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["test", "rr"])),
]}

// This VCD file contains signals that are not initialized at zero and only obtain their first value at a later point.
snapshot_ui_with_file_and_msgs! {vcd_with_non_zero_start_displays_correctly, "examples/gameroy_trace_with_non_zero_start.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["gameroy", "cpu"])),
]}

// This VCD file used to cause Issue 145 (https://gitlab.com/surfer-project/surfer/-/issues/145).
// It contains "false" changes, where a change to the same value is reported.
snapshot_ui_with_file_and_msgs! {vcd_with_false_changes_correctly, "examples/issue_145.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["logic"])),
]}

// This GHW file was generated with GHDL from a simple VHDL test provided by oscar.
snapshot_ui_with_file_and_msgs! {simple_ghw_loads, "examples/oscar_test.ghw", [
    Message::AddScope(ScopeRef::from_strs(&["test"])),
    Message::AddScope(ScopeRef::from_strs(&["test", "rr"])),
]}

// This GHW file comes from the GHDL regression suite.
snapshot_ui_with_file_and_msgs! {ghw_from_ghdl_suite_loads, "examples/tb_recv.ghw", [
    Message::AddScope(ScopeRef::from_strs(&["tb_recv"])),
    Message::AddScope(ScopeRef::from_strs(&["tb_recv", "dut"])),
]}

#[cfg(feature = "spade")]
snapshot_ui_with_file_spade_and_msgs! {
    spade_translation_works,
    "examples/spade.vcd",
    Some("proj::pipeline_ready_valid::ready_valid_pipeline".to_string()),
    Some("examples/spade_state.ron".into()),
    [
    Message::AddScope(ScopeRef::from_strs(&[
        "proj::pipeline_ready_valid::ready_valid_pipeline"
    ])),
    ]
}

snapshot_ui_with_file_and_msgs! {divider_works, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::AddDivider(Some("Divider".to_string()), None),
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::ItemBackgroundColorChange(Some(DisplayedItemIndex(4)), Some("Blue".to_string())),
    Message::ItemColorChange(Some(DisplayedItemIndex(4)), Some("Green".to_string()))
]}

snapshot_ui_with_file_and_msgs! {markers_work, "examples/counter.vcd", [
    Message::ToggleOverview,
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::CursorSet(BigInt::from(600)),
    Message::MoveMarkerToCursor(2),
    Message::ItemColorChange(Some(DisplayedItemIndex(4)), Some("Blue".to_string())),
    Message::CursorSet(BigInt::from(200)),
    Message::MoveMarkerToCursor(1),
    Message::ItemColorChange(Some(DisplayedItemIndex(5)), Some("Green".to_string())),
    Message::CursorSet(BigInt::from(500)),
]}

snapshot_ui_with_file_and_msgs! {markers_dialog_work, "examples/counter.vcd", [
    Message::ToggleOverview,
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::CursorSet(BigInt::from(600)),
    Message::MoveMarkerToCursor(2),
    Message::ItemColorChange(Some(DisplayedItemIndex(4)), Some("Blue".to_string())),
    Message::CursorSet(BigInt::from(200)),
    Message::MoveMarkerToCursor(1),
    Message::ItemColorChange(Some(DisplayedItemIndex(5)), Some("Green".to_string())),
    Message::CursorSet(BigInt::from(500)),
    Message::SetCursorWindowVisible(true)
]}

snapshot_ui_with_file_and_msgs! {goto_markers, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::CursorSet(BigInt::from(600)),
    Message::MoveMarkerToCursor(2),
    Message::GoToMarkerPosition(2, 0)
]}

snapshot_ui_with_file_and_msgs! {
    startup_commands_work,
    "examples/counter.vcd",
    state_mods: (|state: &mut State| {
        state.add_startup_commands(vec!["scope_add tb".to_string()]);
    }),
    []
}

// NOTE: The `divider_add .` command currently fails because of a bug in the CLI
// parsing library. If we fix that, it this test should be updated. For now, it is
// enough to make sure that this one broken command doesn't bring the rest of the
// test down
snapshot_ui_with_file_and_msgs! {
    yosys_blogpost_startup_commands_work,
    "examples/picorv32.vcd",
    state_mods: (|state: &mut State| {
        state.add_startup_commands(vec!["startup_commands=module_add testbench;divider_add .;divider_add top;module_add testbench.top;show_quick_start".to_string()]);
    }),
    []
}

snapshot_ui_with_file_and_msgs! {signals_are_added_at_focus, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(1)),
    Message::AddVariables(vec![VariableRef::from_hierarchy_string("tb.dut.counter")])
]}

snapshot_ui_with_file_and_msgs! {dividers_are_added_at_focus, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(1)),
    Message::AddDivider(Some(String::from("Test")), None)
]}

snapshot_ui_with_file_and_msgs! {dividers_are_appended_without_focus, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::AddDivider(Some(String::from("Test")), None)
]}

snapshot_ui_with_file_and_msgs! {timeline_render, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::AddTimeLine(None)
]}

snapshot_ui_with_file_and_msgs! {toggle_tick_lines, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::ToggleTickLines
]}

snapshot_ui_with_file_and_msgs! {command_prompt, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::ShowCommandPrompt(true)
]}

snapshot_ui_with_file_and_msgs! {command_prompt_next_command, "examples/counter.vcd", [
    Message::ShowCommandPrompt(true),
    Message::CommandPromptUpdate { suggestions: vec![("test".to_string(), vec![true, true, false, false]); 10] },
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
]}

snapshot_ui_with_file_and_msgs! {command_prompt_prev_command, "examples/counter.vcd", [
    Message::ShowCommandPrompt(true),
    Message::CommandPromptUpdate { suggestions: vec![("test".to_string(), vec![true, true, false, false]); 10] },
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectPrevCommand
]}

snapshot_ui_with_file_and_msgs! {command_prompt_scrolls, "examples/counter.vcd", [
    Message::ShowCommandPrompt(true),
    Message::CommandPromptUpdate { suggestions: vec![("test".to_string(), vec![true, true, false, false]); 50] },
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand,
    Message::SelectNextCommand
]}

snapshot_ui_with_file_and_msgs!(
    command_prompt_scroll_bounds_prev,
    "examples/counter.vcd",
    [
        Message::ShowCommandPrompt(true),
        Message::CommandPromptUpdate {
            suggestions: vec![("test".to_string(), vec![true, true, false, false]); 5]
        },
        Message::SelectPrevCommand,
    ]
);

snapshot_ui_with_file_and_msgs!(
    command_prompt_scroll_bounds_next,
    "examples/counter.vcd",
    [
        Message::ShowCommandPrompt(true),
        Message::CommandPromptUpdate {
            suggestions: vec![("test".to_string(), vec![true, true, false, false]); 5]
        },
        // 5 items, 6 "select next command"
        Message::SelectNextCommand,
        Message::SelectNextCommand,
        Message::SelectNextCommand,
        Message::SelectNextCommand,
        Message::SelectNextCommand,
        Message::SelectNextCommand,
    ]
);

snapshot_ui_with_file_and_msgs! {negative_cursorlocation, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::GoToTime(Some(BigInt::from(-50)), 0),
    Message::CursorSet(BigInt::from(-100)),
]}

snapshot_ui_with_file_and_msgs! {goto_start, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::CanvasZoom {mouse_ptr: None, delta:0.2, viewport_idx: 0},
    Message::GoToStart{viewport_idx: 0}
]}

snapshot_ui_with_file_and_msgs! {goto_end, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::CanvasZoom {mouse_ptr: None, delta:0.2, viewport_idx: 0},
    Message::GoToEnd{viewport_idx: 0}
]}

snapshot_ui_with_file_and_msgs! {zoom_to_fit, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::CanvasZoom {mouse_ptr: None, delta:0.2, viewport_idx: 0},
    Message::GoToEnd{viewport_idx: 0},
    Message::ZoomToFit{viewport_idx: 0}
]}

snapshot_ui_with_file_and_msgs! {zoom_to_range, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::ZoomToRange { start: BigInt::from(100), end: BigInt::from(250) , viewport_idx: 0}
]}

snapshot_ui_with_file_and_msgs! {remove_item, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::RemoveItem(DisplayedItemIndex(1), 1)
]}

snapshot_ui_with_file_and_msgs! {remove_items, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::RemoveItem(DisplayedItemIndex(2), 6)
]}

snapshot_ui_with_file_and_msgs! {remove_item_with_focus, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(1)),
    Message::RemoveItem(DisplayedItemIndex(1), 1)
]}

snapshot_ui_with_file_and_msgs! {remove_item_before_focus, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(3)),
    Message::RemoveItem(DisplayedItemIndex(1), 1)
]}

snapshot_ui_with_file_and_msgs! {remove_item_after_focus, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(1)),
    Message::RemoveItem(DisplayedItemIndex(2), 1)
]}

snapshot_ui_with_file_and_msgs! {canvas_scroll, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::CanvasScroll { delta: Vec2 { x: 0., y: 100.}, viewport_idx: 0 }
]}

snapshot_ui_with_file_and_msgs! {move_focused_item_up, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(2)),
    Message::MoveFocusedItem(MoveDir::Up, 1),
]}

snapshot_ui_with_file_and_msgs! {move_focused_item_to_top, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(2)),
    Message::MoveFocusedItem(MoveDir::Up, 4),
]}

snapshot_ui_with_file_and_msgs! {move_focused_item_down, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(0)),
    Message::MoveFocusedItem(MoveDir::Down, 2),
]}

snapshot_ui_with_file_and_msgs! {move_focused_item_to_bottom, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(0)),
    Message::MoveFocusedItem(MoveDir::Down, 10),
]}

snapshot_ui_with_file_and_msgs! {move_focus_up, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(2)),
    Message::MoveFocus(MoveDir::Up, 1),
]}

snapshot_ui_with_file_and_msgs! {move_focus_to_top, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(2)),
    Message::MoveFocus(MoveDir::Up, 4),
]}

snapshot_ui_with_file_and_msgs! {move_focus_down, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(0)),
    Message::MoveFocus(MoveDir::Down, 2),
]}

snapshot_ui_with_file_and_msgs! {move_focus_to_bottom, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(0)),
    Message::MoveFocus(MoveDir::Down, 10),
]}

snapshot_ui!(regex_error_indication, || {
    let mut state = State::new_default_config()
        .unwrap()
        .with_params(StartupParams {
            waves: Some(WaveSource::File(
                get_project_root()
                    .unwrap()
                    .join("examples/counter.vcd")
                    .try_into()
                    .unwrap(),
            )),
            spade_top: None,
            spade_state: None,
            startup_commands: vec![],
        });
    loop {
        state.handle_async_messages();
        state.handle_batch_commands();
        if state.waves_fully_loaded() {
            break;
        }
    }

    let msgs = [
        Message::ToggleMenu,
        Message::ToggleToolbar,
        Message::ToggleOverview,
        Message::SetActiveScope(ScopeRef::from_strs(&["tb"])),
        Message::AddVariables(vec![VariableRef::from_hierarchy_string("tb.clk")]),
        Message::SetVariableNameFilterType(VariableNameFilterType::Regex),
    ];
    for message in msgs.into_iter() {
        state.update(message);
    }
    state.sys.variable_name_filter.borrow_mut().push_str("a(");
    // make sure all the signals added by the proceeding messages are properly loaded
    wait_for_waves_fully_loaded(&mut state, 10);
    state
});

snapshot_ui_with_file_and_msgs! {signal_list_works, "examples/counter.vcd", [
    Message::ToggleSidePanel,
    Message::ToggleDirection,
    Message::SetActiveScope(ScopeRef::from_strs(&["tb"])),
    Message::AddVariables(vec![VariableRef::from_hierarchy_string("tb.clk")]),
]}

snapshot_ui!(fuzzy_signal_filter_works, || {
    let mut state = State::new_default_config()
        .unwrap()
        .with_params(StartupParams {
            waves: Some(WaveSource::File(
                get_project_root()
                    .unwrap()
                    .join("examples/picorv32.vcd")
                    .try_into()
                    .unwrap(),
            )),
            spade_top: None,
            spade_state: None,
            startup_commands: vec![],
        });
    loop {
        state.handle_async_messages();
        state.handle_batch_commands();
        if state.waves_fully_loaded() {
            break;
        }
    }

    let msgs = [
        Message::ToggleMenu,
        Message::ToggleToolbar,
        Message::ToggleOverview,
        Message::ToggleDirection,
        Message::SetActiveScope(ScopeRef::from_strs(&["testbench", "top", "mem"])),
        Message::AddVariables(vec![VariableRef::from_hierarchy_string("testbench.clk")]),
        Message::SetVariableNameFilterType(VariableNameFilterType::Fuzzy),
    ];
    for message in msgs.into_iter() {
        state.update(message);
    }
    state.sys.variable_name_filter.borrow_mut().push_str("at");
    // make sure all the signals added by the proceeding messages are properly loaded
    wait_for_waves_fully_loaded(&mut state, 10);
    state
});

snapshot_ui!(contain_signal_filter_works, || {
    let mut state = State::new_default_config()
        .unwrap()
        .with_params(StartupParams {
            waves: Some(WaveSource::File(
                get_project_root()
                    .unwrap()
                    .join("examples/picorv32.vcd")
                    .try_into()
                    .unwrap(),
            )),
            spade_top: None,
            spade_state: None,
            startup_commands: vec![],
        });
    loop {
        state.handle_async_messages();
        state.handle_batch_commands();
        if state.waves_fully_loaded() {
            break;
        }
    }

    let msgs = [
        Message::ToggleMenu,
        Message::ToggleToolbar,
        Message::ToggleOverview,
        Message::ToggleDirection,
        Message::SetActiveScope(ScopeRef::from_strs(&["testbench", "top", "mem"])),
        Message::AddVariables(vec![VariableRef::from_hierarchy_string("testbench.clk")]),
        Message::SetVariableNameFilterType(VariableNameFilterType::Contain),
    ];
    for message in msgs.into_iter() {
        state.update(message);
    }
    state.sys.variable_name_filter.borrow_mut().push_str("at");
    // make sure all the signals added by the proceeding messages are properly loaded
    wait_for_waves_fully_loaded(&mut state, 10);
    state
});

snapshot_ui!(regex_signal_filter_works, || {
    let mut state = State::new_default_config()
        .unwrap()
        .with_params(StartupParams {
            waves: Some(WaveSource::File(
                get_project_root()
                    .unwrap()
                    .join("examples/picorv32.vcd")
                    .try_into()
                    .unwrap(),
            )),
            spade_top: None,
            spade_state: None,
            startup_commands: vec![],
        });
    loop {
        state.handle_async_messages();
        state.handle_batch_commands();
        if state.waves_fully_loaded() {
            break;
        }
    }

    let msgs = [
        Message::ToggleMenu,
        Message::ToggleToolbar,
        Message::ToggleOverview,
        Message::ToggleDirection,
        Message::SetActiveScope(ScopeRef::from_strs(&["testbench", "top", "mem"])),
        Message::AddVariables(vec![VariableRef::from_hierarchy_string("testbench.clk")]),
        Message::SetVariableNameFilterType(VariableNameFilterType::Regex),
    ];
    for message in msgs.into_iter() {
        state.update(message);
    }
    state
        .sys
        .variable_name_filter
        .borrow_mut()
        .push_str("a[dx]");
    // make sure all the signals added by the proceeding messages are properly loaded
    wait_for_waves_fully_loaded(&mut state, 10);
    state
});

snapshot_ui!(start_signal_filter_works, || {
    let mut state = State::new_default_config()
        .unwrap()
        .with_params(StartupParams {
            waves: Some(WaveSource::File(
                get_project_root()
                    .unwrap()
                    .join("examples/picorv32.vcd")
                    .try_into()
                    .unwrap(),
            )),
            spade_top: None,
            spade_state: None,
            startup_commands: vec![],
        });
    loop {
        state.handle_async_messages();
        state.handle_batch_commands();
        if state.waves_fully_loaded() {
            break;
        }
    }

    let msgs = [
        Message::ToggleMenu,
        Message::ToggleToolbar,
        Message::ToggleOverview,
        Message::ToggleDirection,
        Message::SetActiveScope(ScopeRef::from_strs(&["testbench", "top", "mem"])),
        Message::AddVariables(vec![VariableRef::from_hierarchy_string("testbench.clk")]),
        Message::SetVariableNameFilterType(VariableNameFilterType::Start),
    ];
    for message in msgs.into_iter() {
        state.update(message);
    }
    state.sys.variable_name_filter.borrow_mut().push('a');
    // make sure all the signals added by the proceeding messages are properly loaded
    wait_for_waves_fully_loaded(&mut state, 10);
    state
});

snapshot_ui!(case_sensitive_signal_filter_works, || {
    let mut state = State::new_default_config()
        .unwrap()
        .with_params(StartupParams {
            waves: Some(WaveSource::File(
                get_project_root()
                    .unwrap()
                    .join("examples/picorv32.vcd")
                    .try_into()
                    .unwrap(),
            )),
            spade_top: None,
            spade_state: None,
            startup_commands: vec![],
        });
    loop {
        state.handle_async_messages();
        state.handle_batch_commands();
        if state.waves_fully_loaded() {
            break;
        }
    }

    let msgs = [
        Message::ToggleMenu,
        Message::ToggleToolbar,
        Message::ToggleOverview,
        Message::ToggleDirection,
        Message::SetActiveScope(ScopeRef::from_strs(&["testbench", "top", "mem"])),
        Message::AddVariables(vec![VariableRef::from_hierarchy_string("testbench.clk")]),
        Message::SetVariableNameFilterType(VariableNameFilterType::Start),
        Message::SetVariableNameFilterCaseInsensitive(false),
    ];
    for message in msgs.into_iter() {
        state.update(message);
    }
    state.sys.variable_name_filter.borrow_mut().push('a');
    // make sure all the signals added by the proceeding messages are properly loaded
    wait_for_waves_fully_loaded(&mut state, 10);
    state
});

snapshot_ui!(load_keep_all_works, || {
    let mut state = State::new_default_config()
        .unwrap()
        .with_params(StartupParams {
            waves: Some(WaveSource::File(
                get_project_root()
                    .unwrap()
                    .join("examples")
                    .join("xx_1.vcd")
                    .try_into()
                    .unwrap(),
            )),
            spade_top: None,
            spade_state: None,
            startup_commands: vec![],
        });
    wait_for_waves_fully_loaded(&mut state, 10);

    let msgs = [
        Message::ToggleMenu,
        Message::ToggleToolbar,
        Message::ToggleOverview,
        Message::ToggleSidePanel,
        Message::AddScope(ScopeRef::from_strs(&["TOP"])),
        Message::AddScope(ScopeRef::from_strs(&["TOP", "Foobar"])),
        Message::LoadWaveformFile(
            get_project_root()
                .unwrap()
                .join("examples")
                .join("xx_2.vcd")
                .try_into()
                .unwrap(),
            LoadOptions {
                keep_variables: true,
                keep_unavailable: true,
            },
        ),
    ];
    for message in msgs.into_iter() {
        state.update(message);
    }
    loop {
        state.handle_async_messages();
        state.handle_batch_commands();
        if let Some(waves) = &state.waves {
            if waves.source
                == WaveSource::File(
                    get_project_root()
                        .unwrap()
                        .join("examples")
                        .join("xx_2.vcd")
                        .try_into()
                        .unwrap(),
                )
            {
                break;
            }
        }
    }
    wait_for_waves_fully_loaded(&mut state, 10);
    state
});

snapshot_ui!(load_keep_signal_remove_unavailable_works, || {
    let mut state = State::new_default_config()
        .unwrap()
        .with_params(StartupParams {
            waves: Some(WaveSource::File(
                get_project_root()
                    .unwrap()
                    .join("examples")
                    .join("xx_1.vcd")
                    .try_into()
                    .unwrap(),
            )),
            spade_top: None,
            spade_state: None,
            startup_commands: vec![],
        });
    wait_for_waves_fully_loaded(&mut state, 10);

    let msgs = [
        Message::ToggleMenu,
        Message::ToggleToolbar,
        Message::ToggleOverview,
        Message::ToggleSidePanel,
        Message::AddScope(ScopeRef::from_strs(&["TOP"])),
        Message::AddScope(ScopeRef::from_strs(&["TOP", "Foobar"])),
        Message::LoadWaveformFile(
            get_project_root()
                .unwrap()
                .join("examples")
                .join("xx_2.vcd")
                .try_into()
                .unwrap(),
            LoadOptions {
                keep_variables: true,
                keep_unavailable: false,
            },
        ),
    ];
    for message in msgs.into_iter() {
        state.update(message);
    }
    loop {
        state.handle_async_messages();
        state.handle_batch_commands();
        if let Some(waves) = &state.waves {
            if waves.source
                == WaveSource::File(
                    get_project_root()
                        .unwrap()
                        .join("examples")
                        .join("xx_2.vcd")
                        .try_into()
                        .unwrap(),
                )
            {
                break;
            }
        }
    }
    wait_for_waves_fully_loaded(&mut state, 10);
    state
});

snapshot_ui_with_file_and_msgs! {alignment_right_works, "examples/counter.vcd", [
Message::ToggleOverview,
Message::AddScope(ScopeRef::from_strs(&["tb"])),
Message::SetNameAlignRight(true)
]}

snapshot_ui_with_file_and_msgs! {add_viewport_works, "examples/counter.vcd", [
    Message::AddViewport,
    Message::AddViewport,
    Message::SetActiveScope(ScopeRef::from_strs(&["tb"])),
    Message::AddVariables(vec![VariableRef::from_hierarchy_string("tb.clk")]),
    Message::AddTimeLine(None),
]}

snapshot_ui_with_file_and_msgs! {remove_viewport_works, "examples/counter.vcd", [
    Message::AddViewport,
    Message::AddViewport,
    Message::SetActiveScope(ScopeRef::from_strs(&["tb"])),
    Message::AddVariables(vec![VariableRef::from_hierarchy_string("tb.clk")]),
    Message::AddTimeLine(None), Message::RemoveViewport
]}

snapshot_ui_with_file_and_msgs! {hierarchy_tree, "examples/counter.vcd", [
    Message::ToggleSidePanel,
    Message::SetHierarchyStyle(HierarchyStyle::Tree),
]}

// makes sure that variables that are not part of a scope are properly displayed in the hierarchy tree view
snapshot_ui_with_file_and_msgs! {hierarchy_tree_with_root_vars, "examples/atxmega256a3u-bmda-jtag_short.vcd", [
    Message::ToggleSidePanel,
    Message::ToggleDirection,
    Message::SetHierarchyStyle(HierarchyStyle::Tree),
    Message::AddVariables(vec![
        VariableRef::from_strs(&["tck"]),
        VariableRef::from_strs(&["tms"]),
        VariableRef::from_strs(&["tdi"]),
        VariableRef::from_strs(&["tdo"]),
        VariableRef::from_strs(&["srst"])])
]}

// makes sure that variables that are not part of a scope are properly displayed in the separate hierarchy view
snapshot_ui_with_file_and_msgs! {hierarchy_separate_with_root_vars, "examples/atxmega256a3u-bmda-jtag_short.vcd", [
    Message::ToggleSidePanel,
    Message::ToggleDirection,
    Message::AddVariables(vec![
        VariableRef::from_strs(&["tck"]),
        VariableRef::from_strs(&["tms"]),
        VariableRef::from_strs(&["tdi"]),
        VariableRef::from_strs(&["tdo"]),
        VariableRef::from_strs(&["srst"])])
]}

snapshot_ui_with_file_and_msgs! {hierarchy_separate, "examples/counter.vcd", [
    Message::ToggleSidePanel,
    Message::SetHierarchyStyle(HierarchyStyle::Separate),
]}

snapshot_ui_with_file_and_msgs! {aliasing_works_on_random_3_16, "examples/random_3_16_true.vcd", [
    Message::AddVariables(vec![VariableRef::from_hierarchy_string("TOP.LEB128Compressor_3_16.adaptedCounterFlagBits")]),
]}

snapshot_ui_with_file_and_msgs! {next_transition, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::CursorSet(BigInt::from(500)),
    Message::FocusItem(DisplayedItemIndex(0)),
    Message::MoveCursorToTransition { next: true, variable: None, skip_zero: false }
]}

snapshot_ui_with_file_and_msgs! {next_transition_numbered, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::CursorSet(BigInt::from(500)),
    Message::MoveCursorToTransition { next: true, variable: Some(DisplayedItemIndex(0)), skip_zero: false }
]}

snapshot_ui_with_file_and_msgs! {next_transition_do_not_get_stuck, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::CursorSet(BigInt::from(500)),
    Message::FocusItem(DisplayedItemIndex(0)),
    Message::MoveCursorToTransition { next: true, variable: None, skip_zero: false },
    Message::MoveCursorToTransition { next: true, variable: None, skip_zero: false }
]}

snapshot_ui_with_file_and_msgs! {previous_transition, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::CursorSet(BigInt::from(500)),
    Message::FocusItem(DisplayedItemIndex(0)),
    Message::MoveCursorToTransition { next: false, variable: None, skip_zero: false}
]}

snapshot_ui_with_file_and_msgs! {previous_transition_numbered, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::CursorSet(BigInt::from(500)),
    Message::MoveCursorToTransition { next: false, variable: Some(DisplayedItemIndex(0)), skip_zero: false }
]}

snapshot_ui_with_file_and_msgs! {previous_transition_do_not_get_stuck, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::CursorSet(BigInt::from(500)),
    Message::FocusItem(DisplayedItemIndex(0)),
    Message::MoveCursorToTransition { next: false, variable: None, skip_zero: false },
    Message::MoveCursorToTransition { next: false, variable: None, skip_zero: false }
]}

snapshot_ui_with_file_and_msgs! {next_transition_no_cursor, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(0)),
    Message::MoveCursorToTransition { next: true, variable: None, skip_zero: false },
]}

snapshot_ui_with_file_and_msgs! {previous_transition_no_cursor, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(0)),
    Message::MoveCursorToTransition { next: false, variable: None, skip_zero: false },
]}

snapshot_ui_with_file_and_msgs! {next_transition_skip_zero, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(1)),
    Message::MoveCursorToTransition { next: true, variable: None, skip_zero: true },
    Message::MoveCursorToTransition { next: true, variable: None, skip_zero: true }
]}

snapshot_ui_with_file_and_msgs! {previous_transition_skip_zero, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(1)),
    Message::MoveCursorToTransition { next: false, variable: None, skip_zero: true },
    Message::MoveCursorToTransition { next: false, variable: None, skip_zero: true }
]}

snapshot_ui_with_file_and_msgs! {toggle_variable_indices, "examples/counter.vcd", [
    Message::AddVariables(vec![VariableRef::from_hierarchy_string("tb.dut.counter")]),
    Message::ToggleIndices
]}

snapshot_ui_with_file_and_msgs! {direction_works, "examples/tb_recv.ghw", [
    Message::ToggleSidePanel,
    Message::SetActiveScope(ScopeRef::from_strs(&["tb_recv", "dut"])),
    Message::AddVariables(vec![VariableRef::from_hierarchy_string("tb_recv.dut.en")]),
]}

snapshot_ui!(signals_can_be_added_after_file_switch, || {
    let mut state = State::new_default_config()
        .unwrap()
        .with_params(StartupParams {
            waves: Some(WaveSource::File(
                get_project_root()
                    .unwrap()
                    .join("examples/counter.vcd")
                    .try_into()
                    .unwrap(),
            )),
            spade_top: None,
            spade_state: None,
            startup_commands: vec![],
        });

    wait_for_waves_fully_loaded(&mut state, 10);
    state.update(Message::ToggleToolbar);
    state.update(Message::ToggleMenu);
    state.update(Message::ToggleSidePanel);
    state.update(Message::AddVariables(vec![
        VariableRef::from_hierarchy_string("tb.dut.counter"),
    ]));
    state.update(Message::LoadWaveformFile(
        "examples/counter2.vcd".into(),
        LoadOptions {
            keep_variables: true,
            keep_unavailable: false,
        },
    ));

    loop {
        state.handle_async_messages();
        state.handle_batch_commands();
        if state
            .waves
            .as_ref()
            .is_some_and(|w| w.source.to_string() == "examples/counter2.vcd")
        {
            break;
        }
    }

    state.update(Message::AddVariables(vec![
        VariableRef::from_hierarchy_string("tb.clk"),
    ]));
    state.update(Message::AddVariables(vec![
        VariableRef::from_hierarchy_string("tb.reset"),
    ]));

    wait_for_waves_fully_loaded(&mut state, 10);

    state
});

/// wait for GUI to converge
#[inline]
pub fn wait_for_waves_fully_loaded(state: &mut State, timeout_s: u64) {
    let load_start = std::time::Instant::now();
    while !(state.waves_fully_loaded() && state.batch_commands_completed()) {
        state.handle_async_messages();
        state.handle_batch_commands();
        if load_start.elapsed().as_secs() > timeout_s {
            panic!("Timeout after {timeout_s}s!");
        }
    }
}

snapshot_ui_with_file_and_msgs! {default_dark_theme_works, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(1)),
    Message::MoveCursorToTransition { next: true, variable: None, skip_zero: true },
    Message::MoveCursorToTransition { next: true, variable: None, skip_zero: true },
    Message::SelectTheme(Some("dark+".to_string()))
]}

snapshot_ui_with_file_and_msgs! {default_light_theme_works, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(1)),
    Message::MoveCursorToTransition { next: true, variable: None, skip_zero: true },
    Message::MoveCursorToTransition { next: true, variable: None, skip_zero: true },
    Message::SelectTheme(Some("light+".to_string()))
]}

snapshot_ui_with_file_and_msgs! {default_solarized_theme_works, "examples/counter.vcd", [
    Message::AddScope(ScopeRef::from_strs(&["tb"])),
    Message::FocusItem(DisplayedItemIndex(1)),
    Message::MoveCursorToTransition { next: true, variable: None, skip_zero: true },
    Message::MoveCursorToTransition { next: true, variable: None, skip_zero: true },
    Message::SelectTheme(Some("solarized".to_string()))
]}

snapshot_ui_with_file_and_msgs! {undo_redo_works, "examples/counter.vcd", [
    Message::AddVariables(vec![]),
    Message::AddVariables(vec![
        VariableRef::from_hierarchy_string("tb.dut.counter"),
        VariableRef::from_hierarchy_string("tb.dut.clk")]),
    Message::Undo(1),
    Message::Redo(1),
    Message::Undo(1),
    Message::AddVariables(vec![VariableRef::from_hierarchy_string("tb.dut.reset")]),
    Message::AddVariables(vec![VariableRef::from_hierarchy_string("tb.dut.reset")]),
    Message::AddVariables(vec![VariableRef::from_hierarchy_string("tb.dut.reset")]),
    Message::Undo(2),
    Message::AddVariables(vec![VariableRef::from_hierarchy_string("tb.dut.reset")]),
    // the redo stack is cleared when something is added to the view
    Message::Redo(1)
]}

snapshot_ui!(rising_clock_markers, || {
    let mut state = State::new_default_config()
        .unwrap()
        .with_params(StartupParams {
            waves: Some(WaveSource::File(
                get_project_root()
                    .unwrap()
                    .join("examples/counter.vcd")
                    .try_into()
                    .unwrap(),
            )),
            spade_top: None,
            spade_state: None,
            startup_commands: vec![],
        });
    loop {
        state.handle_async_messages();
        state.handle_batch_commands();
        if state.waves_fully_loaded() {
            break;
        }
    }
    state.config.theme.clock_rising_marker = true;
    state.update(Message::ToggleMenu);
    state.update(Message::ToggleSidePanel);
    state.update(Message::ToggleToolbar);
    state.update(Message::ToggleOverview);
    state.update(Message::AddVariables(vec![
        VariableRef::from_hierarchy_string("tb.clk"),
    ]));
    state.update(Message::VariableFormatChange(
        DisplayedFieldRef {
            item: DisplayedItemRef(1),
            field: vec![],
        },
        String::from("Clock"),
    ));
    state.update(Message::CanvasZoom {
        mouse_ptr: None,
        delta: 0.5,
        viewport_idx: 0,
    });
    wait_for_waves_fully_loaded(&mut state, 10);
    state
});