//! Functions run when entering, updating, and leaving [`AppState::Game`].

use bevy::prelude::*;
use bevy_egui::egui;

use crate::{common::ClientCommon, AppState};

pub(crate) fn update(
	mut core: ClientCommon,
	mut _next_state: ResMut<NextState<AppState>>,
	mut cameras: Query<&mut Transform, With<Camera>>,
) {
	let mut cam_speed = 0.1;

	if core.input.keys_virt.pressed(KeyCode::ShiftLeft) {
		cam_speed *= 5.0;
	}

	let mut camera = cameras.get_single_mut().unwrap();

	let mut new_pos = camera.translation;

	if core.input.keys_virt.pressed(KeyCode::W) {
		new_pos += camera.forward() * cam_speed;
	} else if core.input.keys_virt.pressed(KeyCode::S) {
		new_pos += camera.back() * cam_speed;
	}

	if core.input.keys_virt.pressed(KeyCode::A) {
		new_pos += camera.left() * cam_speed;
	} else if core.input.keys_virt.pressed(KeyCode::D) {
		new_pos += camera.right() * cam_speed;
	}

	if core.input.keys_virt.pressed(KeyCode::Q) {
		new_pos += camera.up() * cam_speed;
	} else if core.input.keys_virt.pressed(KeyCode::E) {
		new_pos += camera.down() * cam_speed;
	}

	camera.translation = new_pos;

	camera.rotate_local_x((core.input.cursor_pos.y - core.input.cursor_pos_prev.y) * 0.005);
	camera.rotate_local_y((core.input.cursor_pos.x - core.input.cursor_pos_prev.x) * -0.005);

	if core.input.keys_virt.pressed(KeyCode::Z) {
		camera.rotate_local_z(0.1);
	} else if core.input.keys_virt.pressed(KeyCode::C) {
		camera.rotate_local_z(-0.1);
	}

	egui::Window::new("")
		.id("viletech_devoverlay_pos".into())
		.title_bar(false)
		.show(core.egui.ctx_mut(), |ui| {
			ui.label(format!(
				"{} {} {}",
				camera.translation.x, camera.translation.y, camera.translation.z
			));
		});

	core.draw_devgui();
}

pub(crate) fn on_enter() {
	// TODO: add `Sim` resource.
}

pub(crate) fn on_exit(_: Commands) {
	// TODO: remove `Sim` resource.
}
