mod actions;
mod ask;
mod commands;
#[cfg(test)]
mod conf_guard;
mod diag;
mod dictionary;
mod downloads;
mod general;
mod hotkeys;
mod language;
mod managed;
mod meeting;
mod mic;
mod modes;
mod onboarding;
mod personalization;
mod pipeline;
mod recording;
mod result;
mod retention;
mod secrets;
mod settings;
mod stats;
mod translate;
mod tray;
mod updates;
mod voice;
mod websearch;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(voice::VoiceState::default())
        .manage(meeting::MeetingState::default())
        .manage(managed::Managed::default())
        .setup(|app| {
            tray::setup(app)?;
            let _ = crate::retention::purge_expired();
            if let Err(e) = voice::register_all(app.handle()) {
                pipeline::emit(
                    app.handle(),
                    "failed",
                    0.0,
                    None,
                    Some(format!("hotkeys: {e}")),
                );
            }
            voice::start_permission_watch(app.handle().clone());
            if let Some(recipe) = settings::load().managed_recipe {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<managed::Managed>();
                    if let Err(e) = state.start_recipe(&handle, &recipe).await {
                        pipeline::emit(
                            &handle,
                            "failed",
                            0.0,
                            None,
                            Some(format!("local models: {e}")),
                        );
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_status,
            commands::list_recipes,
            commands::list_models,
            commands::list_history,
            actions::save_connection,
            actions::test_connection,
            actions::download_model,
            meeting::start_meeting,
            meeting::stop_meeting,
            meeting::list_meetings,
            meeting::get_meeting,
            meeting::delete_meeting,
            meeting::meeting_markdown,
            managed::use_recipe,
            onboarding::system_info,
            onboarding::permission_status,
            onboarding::open_privacy_settings,
            onboarding::request_accessibility,
            dictionary::list_dictionary,
            dictionary::add_dictionary_entry,
            dictionary::set_dictionary_enabled,
            dictionary::delete_dictionary_entry,
            hotkeys::get_hotkeys,
            hotkeys::set_hotkeys,
            hotkeys::hotkey_status,
            retention::delete_all_history,
            modes::get_modes,
            modes::save_mode_prompt,
            modes::test_mode,
            mic::list_mics,
            mic::set_mic,
            mic::test_mic,
            general::get_general,
            general::set_general,
            diag::run_diagnostics,
            hotkeys::suspend_hotkeys,
            hotkeys::resume_hotkeys,
            ask::replace_selection,
            result::result_insert,
            result::result_retry,
            downloads::list_downloads,
            downloads::pause_download,
            downloads::resume_download,
            downloads::cancel_download,
            stats::get_stats,
            translate::get_translate_targets,
            translate::set_translate_targets,
            websearch::get_search,
            websearch::save_search,
            websearch::test_search,
            personalization::get_style,
            personalization::save_style,
            personalization::learn_style,
            updates::check_update,
            updates::install_update,
            language::get_ui_language,
            language::set_ui_language
        ])
        .run(tauri::generate_context!())
        .expect("error while running Launch");
}
