use std::io::IsTerminal;

use pattern::{
    backend::{Backend, EventLoop},
    config::ConfigManager,
    input::Input,
    server::Composer,
    styler,
    wm::impls::floating_wm,
};
use tracing::{error, info};
use wayland_protocols::ext::workspace::v1::server::ext_workspace_manager_v1::ExtWorkspaceManagerV1;
use wayland_protocols::wp::cursor_shape::v1::server::wp_cursor_shape_manager_v1::WpCursorShapeManagerV1;
use wayland_protocols::wp::fifo::v1::server::wp_fifo_manager_v1::WpFifoManagerV1;
use wayland_protocols::wp::linux_dmabuf::zv1::server::zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1;
use wayland_protocols::wp::linux_drm_syncobj::v1::server::wp_linux_drm_syncobj_manager_v1::WpLinuxDrmSyncobjManagerV1;
use wayland_protocols::wp::pointer_constraints::zv1::server::zwp_pointer_constraints_v1::ZwpPointerConstraintsV1;
use wayland_protocols::wp::pointer_gestures::zv1::server::zwp_pointer_gestures_v1::ZwpPointerGesturesV1;
use wayland_protocols::wp::pointer_warp::v1::server::wp_pointer_warp_v1::WpPointerWarpV1;
use wayland_protocols::wp::presentation_time::server::wp_presentation::WpPresentation;
use wayland_protocols::wp::primary_selection::zv1::server::zwp_primary_selection_device_manager_v1::ZwpPrimarySelectionDeviceManagerV1;
use wayland_protocols::wp::relative_pointer::zv1::server::zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1;
use wayland_protocols::wp::text_input::zv3::server::zwp_text_input_manager_v3::ZwpTextInputManagerV3;
use wayland_protocols::wp::viewporter::server::wp_viewporter::WpViewporter;
use wayland_protocols::xdg::activation::v1::server::xdg_activation_v1::XdgActivationV1;
use wayland_protocols::xdg::decoration::zv1::server::zxdg_decoration_manager_v1::ZxdgDecorationManagerV1;
use wayland_protocols::xdg::dialog::v1::server::xdg_wm_dialog_v1::XdgWmDialogV1;
use wayland_protocols::xdg::shell::server::xdg_wm_base::XdgWmBase;
use wayland_protocols::xdg::xdg_output::zv1::server::zxdg_output_manager_v1::ZxdgOutputManagerV1;
use wayland_protocols_misc::zwp_input_method_v2::server::zwp_input_method_manager_v2::ZwpInputMethodManagerV2;
use wayland_protocols_misc::zwp_virtual_keyboard_v1::server::zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1;
use wayland_protocols_wlr::data_control::v1::server::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1;
use wayland_protocols_wlr::gamma_control::v1::server::zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1;
use wayland_protocols_wlr::layer_shell::v1::server::zwlr_layer_shell_v1::ZwlrLayerShellV1;
use wayland_server::{
    Display, ListeningSocket,
    protocol::{
        wl_compositor::WlCompositor, wl_data_device_manager::WlDataDeviceManager,
        wl_seat::WlSeat, wl_shm::WlShm, wl_subcompositor::WlSubcompositor,
    },
};

fn main() {
    unsafe {
        libc::signal(libc::SIGCHLD, libc::SIG_IGN);
    }

    let is_tty = std::io::stdout().is_terminal();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_thread_ids(true)
        .with_ansi(is_tty)
        .with_timer(tracing_subscriber::fmt::time::time())
        .init();

    let mut backend = Backend::new();
    let table_fd = backend.table_fd.take().expect("Table FD already taken");

    info!("Booting Wayland Server");
    let mut display: Display<Composer> = Display::new().unwrap();
    let dh = display.handle();

    dh.create_global::<Composer, WlCompositor, ()>(5, ());
    dh.create_global::<Composer, WlShm, ()>(1, ());
    dh.create_global::<Composer, WlSubcompositor, ()>(1, ());
    dh.create_global::<Composer, WlSeat, ()>(5, ());
    dh.create_global::<Composer, WlDataDeviceManager, ()>(3, ());
    dh.create_global::<Composer, ZwpPrimarySelectionDeviceManagerV1, ()>(1, ());
    dh.create_global::<Composer, XdgWmBase, ()>(3, ());
    dh.create_global::<Composer, ZwpLinuxDmabufV1, ()>(5, ());
    dh.create_global::<Composer, ZxdgDecorationManagerV1, ()>(1, ());
    dh.create_global::<Composer, WpViewporter, ()>(1, ());
    dh.create_global::<Composer, ZxdgOutputManagerV1, ()>(2, ());
    dh.create_global::<Composer, ZwlrLayerShellV1, ()>(4, ());
    dh.create_global::<Composer, ExtWorkspaceManagerV1, ()>(1, ());
    dh.create_global::<Composer, XdgWmDialogV1, ()>(1, ());
    dh.create_global::<Composer, XdgActivationV1, ()>(1, ());
    dh.create_global::<Composer, ZwpPointerGesturesV1, ()>(3, ());
    dh.create_global::<Composer, WpCursorShapeManagerV1, ()>(2, ());
    dh.create_global::<Composer, WpPointerWarpV1, ()>(1, ());
    dh.create_global::<Composer, ZwpPointerConstraintsV1, ()>(1, ());
    dh.create_global::<Composer, ZwpRelativePointerManagerV1, ()>(1, ());
    dh.create_global::<Composer, WpLinuxDrmSyncobjManagerV1, ()>(1, ());
    dh.create_global::<Composer, WpFifoManagerV1, ()>(1, ());
    dh.create_global::<Composer, WpPresentation, ()>(2, ());
    dh.create_global::<Composer, ZwpTextInputManagerV3, ()>(1, ());
    dh.create_global::<Composer, ZwpInputMethodManagerV2, ()>(1, ());
    dh.create_global::<Composer, ZwpVirtualKeyboardManagerV1, ()>(1, ());
    dh.create_global::<Composer, ZwlrDataControlManagerV1, ()>(2, ());
    dh.create_global::<Composer, ZwlrGammaControlManagerV1, ()>(1, ());

    let socket = ListeningSocket::bind_auto("wayland", 0..32).unwrap();
    let socket_name = socket.socket_name().unwrap().to_string_lossy().into_owned();
    info!("Wayland socket created: {}", socket_name);

    unsafe {
        std::env::set_var("WAYLAND_DISPLAY", &socket_name);
        std::env::set_var("XDG_SESSION_TYPE", "wayland");
        std::env::set_var("XDG_CURRENT_DESKTOP", "Pattern");
        std::env::set_var("DESKTOP", "Pattern");
        // std::env::set_var("DISPLAY", ":0"); TODO: XWayland
    }

    let mut config_manager = ConfigManager::new(None).expect("Unable to activate the manager");
    config_manager.load().expect("Unable to load configuration");

    let mut composer = Composer::init(
        &dh,
        backend.vkctx.clone(),
        backend.outputs.clone(),
        backend.gpu_dev_t,
        table_fd,
        Box::new(floating_wm::Wm::new()),
        Box::new(styler::DefaultStyler::new()),
        config_manager,
    );

    let initial_style = {
        let cfg = composer.config_manager.config.lock().unwrap();
        cfg.style.clone()
    };
    composer.styler.update_style(initial_style);

    composer
        .config_manager
        .run_hook("@start")
        .unwrap_or_else(|e| {
            error!("Failed to run @start hook: {:?}", e);
        });

    let mut input = Input::new(
        backend.seat.clone(),
        backend.width as f64,
        backend.height as f64,
    );
    input.natural_scroll = {
        let cfg = composer.config_manager.config.lock().unwrap();
        cfg.input.natural_scroll
    };

    let mut event_loop = EventLoop::new(
        &backend.gbm,
        &input,
        backend.seat.clone(),
        &socket,
        &mut display,
    );

    event_loop
        .run(
            &mut backend,
            &mut display,
            &mut composer,
            &mut input,
            &socket,
        )
        .unwrap();

    drop(composer);
    drop(backend);

    info!("Engine shut down safely. Returning to the terminal.");
}
