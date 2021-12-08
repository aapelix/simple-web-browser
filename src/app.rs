/*
 * Author: Dylan Turner
 * Description: Defines application state
 */

use std::future::Future;
use async_channel::{ unbounded, Sender };
use gtk::{
    main_quit, Inhibit, init, main,
    Button, Box, Orientation, TextView, Grid, TextBuffer, Label,
    Menu, MenuItem, MenuButton,
    Window, WindowType, Align, Dialog, DialogFlags, ResponseType,
    prelude::{
        ContainerExt, ButtonExt, BoxExt, WidgetExt, GtkWindowExt, GridExt,
        TextBufferExt, MenuButtonExt, MenuShellExt, GtkMenuItemExt,
        DialogExt
    }, glib::{ set_program_name, set_application_name, MainContext }
};
use webkit2gtk::{ WebView, LoadEvent, traits::WebViewExt };
use serde::{ Serialize, Deserialize };
use log::{ warn, error, info };
use confy::{ load, store };
use cascade::cascade;
use home::home_dir;

const WIN_TITLE: &'static str = "Browse the Web";
const WIN_DEF_WIDTH: i32 = 640;
const WIND_DEF_HEIGHT: i32 = 480;
const APP_NAME: &'static str = "swb";

// Spawns a task on default executor, without waiting to complete
fn spawn<F>(future: F) where F: Future<Output = ()> + 'static {
    MainContext::default().spawn_local(future);
}

pub fn start_browser() {
    set_program_name(APP_NAME.into());
    set_application_name(APP_NAME);

    // Initialize gtk
    if init().is_err() {
        error!("Failed to initialize GTK Application!");
        panic!("Failed to initialize GTK Application!");
    }

    // Attach tx to widgets and rx to handler
    let (tx, rx) = unbounded();
    let app = AppState::new(tx);

    let mut via_nav_btns = false;
    let mut back_urls = vec![ app.cfg.start_page ];
    let mut fwd_urls = Vec::new();

    let mut err_url = String::new();

    let event_handler = async move {
        while let Ok(event) = rx.recv().await {
            match event.tp {
                EventType::BackClicked => {
                    if back_urls.len() > 1 {
                        fwd_urls.push(back_urls.pop());

                        via_nav_btns = true;
                        app.web_view.load_uri(
                            back_urls[back_urls.len() - 1].as_str()
                        );

                        info!("Back to {}.", back_urls[back_urls.len() - 1]);

                        app.tb_buff.set_text(
                            back_urls[back_urls.len() - 1].as_str()
                        );
                    }
                }, EventType::ForwardClicked => {
                    if fwd_urls.len() > 0 {
                        back_urls.push(fwd_urls[0].clone().unwrap());
                        fwd_urls.remove(0);

                        via_nav_btns = true;
                        app.web_view.load_uri(
                            back_urls[back_urls.len() - 1].as_str()
                        );

                        info!("Forward to {}.", back_urls[back_urls.len() - 1]);

                        app.tb_buff.set_text(
                            back_urls[back_urls.len() - 1].as_str()
                        );
                    }
                }, EventType::RefreshClicked => {
                    via_nav_btns = true;
                    app.web_view.reload();
                }, EventType::ChangedPage => {
                    // Don't re-navigate after pressing back
                    if via_nav_btns {
                        via_nav_btns = false;
                        continue;
                    }

                    info!("Changed page to {}.", event.url);

                    fwd_urls = Vec::new();
                    back_urls.push(event.url.clone());

                    app.tb_buff.set_text(event.url.as_str());
                }, EventType::ChangePage => {
                    app.web_view.load_uri(&event.url);
                }, EventType::FailedChangePage => {
                    if event.url == err_url {
                        let home_dir =
                            home_dir().unwrap().display().to_string();
                        app.web_view.load_uri(
                            (String::from("file://") + &home_dir).as_str()
                        );
                    } else {
                        err_url =
                            app.cfg.search_engine.replace("${}", &event.url);
                        app.web_view.load_uri(err_url.as_str());
                    }
                }, EventType::LoginRegister => {
                    /* Create a login prompt */
                    let dialog = cascade! {
                        Dialog::with_buttons(
                            Some("Sign In"), Some(&app.win),
                            DialogFlags::from_bits(1).unwrap(),
                            &[ ("_OK", ResponseType::Accept) ]
                        );
                        ..connect_response(move |view, _| {
                            view.hide();
                        });
                    };
                    let content_area = dialog.content_area();

                    let uname_buff = TextBuffer::builder().build();
                    let uname = cascade! {
                        Box::new(Orientation::Horizontal, 0);
                            ..pack_start(
                                &Label::new(Some("Username: ")),
                                false, false, app.cfg.margin
                            );..pack_start(
                                &TextView::builder()
                                    .hexpand(true).buffer(&uname_buff).build(),
                                true, true, app.cfg.margin
                            );
                            ..set_expand(true);
                    };
                    
                    // TODO: Finish

                    dialog.show_all();
                }
            }
        }
    };
    MainContext::default().spawn_local(event_handler);

    main();
}

#[derive(Serialize, Deserialize)]
struct AppConfig {
    pub start_page: String,
    pub search_engine: String,
    pub local: bool,
    pub bookmarks: Vec<Vec<Vec<String>>>,
    pub username: String,
    pub pass_enc: String,
    pub margin: u32
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            start_page: String::from("https://duckduckgo.org"),
            search_engine: String::from("https://duckduckgo.com/?q=${}"),
            local: false,
            bookmarks: Vec::new(),
            username: String::new(),
            pass_enc: String::new(),
            margin: 10
        }
    }
}

enum EventType {
    BackClicked,
    ForwardClicked,
    RefreshClicked,
    ChangedPage,
    ChangePage,
    FailedChangePage,
    LoginRegister
}

struct Event {
    pub tp: EventType,
    pub url: String
}

struct AppState {
    pub win: Window,
    pub web_view: WebView,
    pub cfg: AppConfig,
    pub tb_buff: TextBuffer
}

impl AppState {
    pub fn new(tx: Sender<Event>) -> Self {
        // Try to sync bookmarks online
        let mut temp_cfg = match load(APP_NAME) {
            Err(_) => {
                warn!("Error in config! Using defaults.");
                AppConfig::default()
            }, Ok(config) => config
        };
        if !temp_cfg.local {
            // Sync via db
            let synced_bm = Vec::new();

            if false {
                temp_cfg.bookmarks = synced_bm.clone();
                store(APP_NAME, temp_cfg).unwrap();
            }
        }

        // Load config file
        let cfg = match load(APP_NAME) {
            Err(_) => {
                warn!("Error in config! Using defaults.");
                AppConfig::default()
            }, Ok(config) => config
        };
        let start_page = cfg.start_page.clone();

        /* Create navigation bar */

        // Back button
        let back_tx = tx.clone();
        let back_btn = cascade! {
            Button::with_label("←");
                ..set_border_width(cfg.margin);
                ..connect_clicked(move |_| {
                    let tx = back_tx.clone();
                    spawn(async move {
                        let _ = tx.send(Event {
                            tp: EventType::BackClicked, url: String::new()
                        }).await;
                    });
                });
        };

        // Forward button
        let fwd_tx = tx.clone();
        let fwd_btn = cascade! {
            Button::with_label("→");
                ..set_border_width(cfg.margin);
                ..connect_clicked(move |_| {
                    let tx = fwd_tx.clone();
                    spawn(async move {
                        let _ = tx.send(Event {
                            tp: EventType::ForwardClicked, url: String::new()
                        }).await;
                    });
                });
        };

        // Search/Navigation text box
        let buff_tx = tx.clone();
        let buff = cascade! {
            TextBuffer::builder().text(&start_page).build();
                ..connect_changed(move |tb_buff| {
                    let tx = buff_tx.clone();
                    if tb_buff.line_count() > 1 {
                        let txt = match tb_buff.text(
                            &tb_buff.start_iter(), &tb_buff.end_iter(), true
                        ) {
                            None => String::new(),
                            Some(val) => val.to_string()
                        };
                        
                        let lines = txt.split("\n");
                        let val: String = lines.collect();
                        tb_buff.set_text(&val);

                        spawn(async move {
                            let _ = tx.send(Event {
                                tp: EventType::ChangePage,
                                url: val
                            }).await;
                        });
                    }
                });
        };
        let tb =
            TextView::builder().hexpand(true).accepts_tab(false)
                .valign(Align::Center).buffer(&buff).build();

        // Generate book marks menu
        let bookmark_menu = Menu::builder().build();
        for folder in cfg.bookmarks.clone() {
            match folder.len() {
                0 => { },
                1 => {
                    // Lots of clones bc closure expects static. Wouldn't touch
                    let bm = folder[0].clone();
                    let name = bm[0].clone();
                    let bm_url = bm[0].clone();

                    info!("Found local bookmark: {} -> '{}'.", name, bm_url);

                    let item_tx = tx.clone();
                    let item = cascade! {
                        MenuItem::with_label(name.as_str());
                            ..connect_activate(move |_| {
                                let tx = item_tx.clone();
                                let url = bm_url.clone();
                                spawn(async move {
                                    let _ = tx.send(Event {
                                        tp: EventType::ChangePage,
                                        url
                                    }).await;
                                });
                            });
                    };
                    bookmark_menu.append(&item);
                }, _ => {
                    let fldr_name = folder[0][0].clone();
                    let sub_menu = Menu::builder().build();

                    for i in 1..folder.len() {
                        let fldr_clone = folder.clone();
                        let bookmark = fldr_clone[i].clone();

                        let name = bookmark[0].clone();
                        let bm_url = bookmark[1].clone();

                        info!(
                            "Found local bookmark: {}/{} -> '{}'.",
                            fldr_name, name, bm_url
                        );

                        let item_tx = tx.clone();
                        let item = cascade! {
                            MenuItem::with_label(name.as_str());
                                ..connect_activate(move |_| {
                                    let tx = item_tx.clone();
                                    let url = bm_url.clone();
                                    spawn(async move {
                                        let _ = tx.send(Event {
                                            tp: EventType::ChangePage,
                                            url
                                        }).await;
                                    });
                                });
                        };
                        sub_menu.append(&item);
                    }

                    sub_menu.show_all();
                    let item = cascade! {
                        MenuItem::with_label(fldr_name.as_str());
                            ..set_submenu(Some(&sub_menu));
                    };
                    bookmark_menu.append(&item);
                }
            }
        }
        bookmark_menu.show_all();
        let bm_btn = cascade! {
            MenuButton::builder().label("@").build();
                ..set_border_width(cfg.margin);
                ..set_popup(Some(&bookmark_menu));
        };

        let refr_tx = tx.clone();
        let refr_btn = cascade! {
            Button::with_label("↺");
                ..set_border_width(cfg.margin);
                ..connect_clicked(move |_| {
                    let tx = refr_tx.clone();
                    spawn(async move {
                        let _ = tx.send(Event {
                            tp: EventType::RefreshClicked, url: String::new()
                        }).await;
                    });
                });
        };

        /* Create page view */
        let web_tx1 = tx.clone();
        let web_tx2 = tx.clone();
        let web_view = cascade! {
            WebView::builder().build();
                ..load_uri(&start_page);
                ..connect_load_changed(move |view, load_ev| {
                    if load_ev == LoadEvent::Started {
                        let tx = web_tx1.clone();
                        let txt = WebView::uri(&view).unwrap().to_string();
                        spawn(async move {
                            let _ = tx.send(Event {
                                tp: EventType::ChangedPage,
                                url: txt
                            }).await;
                        });
                    }
                });
                ..connect_load_failed(move |_, _, uri, _| {
                    let tx = web_tx2.clone();
                    let url = String::from(uri);
                    spawn(async move {
                        let _ = tx.send(Event {
                            tp: EventType::FailedChangePage,
                            url
                        }).await;
                    });
                    true
                });
        };
        let web_box = cascade! {
            Box::new(Orientation::Horizontal, 0);
                ..pack_start(&web_view, true, true, cfg.margin);
        };

        /* Put it all together */
        let view_cont = cascade! {
            Grid::builder().build();
                ..attach(&back_btn, 0, 0, 1, 1);
                ..attach(&fwd_btn, 1, 0, 1, 1);
                ..attach(&tb, 2, 0, 5, 1);
                ..attach(&bm_btn, 7, 0, 1, 1);
                ..attach(&refr_btn, 8, 0, 1, 1);
        };

        // Sync popup button
        if cfg.local {
            let sync_tx = tx.clone();
            let sync_btn = cascade! {
                Button::with_label("↨");
                    ..set_border_width(cfg.margin);
                    ..connect_clicked(move |_| {
                        let tx = sync_tx.clone();
                        spawn(async move {
                            let _ = tx.send(Event {
                                tp: EventType::LoginRegister,
                                url: String::new()
                            }).await;
                        });
                    });
            };
            view_cont.attach(&sync_btn, 9, 0, 1, 1);
        }

        let view = cascade! {
            Box::new(Orientation::Vertical, 0);
                ..pack_start(&view_cont, false, false, 0);
                ..pack_end(&web_box, true, true, cfg.margin);
        };
        let win = cascade! {
            Window::new(WindowType::Toplevel);
                ..add(&view);
                ..set_title(WIN_TITLE);
                ..set_default_size(WIN_DEF_WIDTH, WIND_DEF_HEIGHT);
                ..connect_delete_event(move |_, _| {
                    main_quit();
                    Inhibit(false)
                });
                ..show_all();
        };
        //gtk::Window::set_default_icon_name("icon-name-here");

        Self {
            win,
            web_view,
            cfg,
            tb_buff: buff
        }
    }
}
