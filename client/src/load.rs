//! Functions run when entering, updating, and leaving [`crate::AppState::Load`].

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use viletech::data::LoadOutcome;

use crate::{
	core::{ClientCore, GameLoad},
	AppState,
};

pub fn update(
	mut core: ResMut<ClientCore>,
	mut next_state: ResMut<NextState<AppState>>,
	mut loader: ResMut<GameLoad>,
	mut ctxs: EguiContexts,
) {
	// TODO: Localize these strings.

	let m_pct = loader.tracker.mount_progress_percent() * 100.0;
	let p_pct = loader.tracker.pproc_progress_percent() * 100.0;
	let mut cancelled = false;

	egui::Window::new("Loading...")
		.id(egui::Id::new("vile_gameload"))
		.show(ctxs.ctx_mut(), |ui| {
			ui.label(&format!("File Mounting: {m_pct:.1}%"));
			ui.label(&format!("Processing: {p_pct:.1}%"));

			if ui.button("Cancel").clicked() {
				cancelled = true;
			}
		});

	core.draw_devgui(ctxs.ctx_mut());

	if cancelled {
		next_state.set(AppState::Frontend);
		return;
	}

	if !loader.tracker.mount_done() || !loader.tracker.pproc_done() {
		return;
	}

	let res_join = loader.thread.take().unwrap().join();

	let res_load = match res_join {
		Ok(results) => results,
		Err(_) => {
			next_state.set(AppState::Frontend);
			error!("Failed to join loader thread.");
			return;
		}
	};

	let failed = match &res_load {
		LoadOutcome::MountFail { errors } => {
			for (i, (real_path, _)) in loader.load_order.iter().enumerate() {
				let num_errs = res_load.num_errs();
				let mut msg = String::with_capacity(128 + 256 * num_errs);

				msg.push_str(&format!(
					"{num_errs} errors/warnings while loading: {}",
					real_path.display()
				));

				msg.push_str("\r\n\r\n");

				for err in &errors[i] {
					msg.push_str(&err.to_string());
				}

				error!("{msg}");
			}

			true
		}
		LoadOutcome::PostProcFail { errors } => {
			for (i, (real_path, _)) in loader.load_order.iter().enumerate() {
				let num_errs = res_load.num_errs();
				let mut msg = String::with_capacity(128 + 256 * num_errs);

				msg.push_str(&format!(
					"{num_errs} errors/warnings while loading: {}",
					real_path.display()
				));

				msg.push_str("\r\n\r\n");

				for err in &errors[i] {
					msg.push_str(&err.to_string());
				}

				error!("{msg}");
			}

			true
		}
		LoadOutcome::Ok { mount, pproc } => {
			for (i, (real_path, _)) in loader.load_order.iter().enumerate() {
				let num_errs = res_load.num_errs();
				let mut msg = String::with_capacity(128 + 256 * num_errs);

				msg.push_str(&format!(
					"{num_errs} errors/warnings while loading: {}",
					real_path.display()
				));

				msg.push_str("\r\n\r\n");

				for err in &mount[i] {
					msg.push_str(&err.to_string());
				}

				for err in &pproc[i] {
					msg.push_str(&err.to_string());
				}

				warn!("{msg}");
			}

			false
		}
	};

	if failed {
		next_state.set(AppState::Frontend);
	} else {
		next_state.set(AppState::Game);
	}
}

pub fn on_exit(mut cmds: Commands) {
	cmds.remove_resource::<GameLoad>();
}