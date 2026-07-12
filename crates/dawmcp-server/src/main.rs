use std::sync::Arc;

use anyhow::Result;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    handler::server::router::tool::ToolRouter,
    transport::stdio,
};
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

use dawmcp_core::{DawBackend, Fx, RenderRequest, Tracks};
use dawmcp_reaper::{default_bridge_dir, install_bridge, run_discovery, BridgeClient, ComposeNote, ReaperBackend};

fn ok_json<T: Serialize>(value: T) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string(&value)
        .map_err(|e| McpError::internal_error(format!("serializing result: {e}"), None))?;
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

fn map_err<T>(result: dawmcp_core::DawResult<T>) -> Result<T, McpError> {
    result.map_err(|e| McpError::internal_error(e.to_string(), None))
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TrackIndexParams {
    track_index: i32,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TrackAddParams {
    name: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TrackRenameParams {
    track_index: i32,
    name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TrackVolumeParams {
    track_index: i32,
    db: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TrackPanParams {
    track_index: i32,
    pan: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TrackMuteParams {
    track_index: i32,
    mute: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TrackSoloParams {
    track_index: i32,
    solo: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TrackColorParams {
    track_index: i32,
    r: u8,
    g: u8,
    b: u8,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SeekParams {
    position_seconds: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TempoParams {
    bpm: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FxAddParams {
    track_index: i32,
    fx_name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FxIndexParams {
    track_index: i32,
    fx_index: i32,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FxEnabledParams {
    track_index: i32,
    fx_index: i32,
    enabled: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FxParamGetParams {
    track_index: i32,
    fx_index: i32,
    param_index: i32,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct FxParamSetParams {
    track_index: i32,
    fx_index: i32,
    param_index: i32,
    value: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RenderParams {
    output_path: String,
    start_seconds: Option<f64>,
    end_seconds: Option<f64>,
    #[serde(default)]
    overwrite: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct MidiAddItemParams {
    track_index: i32,
    start_sec: f64,
    end_sec: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct MidiAddNoteParams {
    track_index: i32,
    item_start_sec: f64,
    pitch: i32,
    velocity: i32,
    note_start_sec: f64,
    note_end_sec: f64,
    channel: Option<i32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ItemSplitParams {
    track_index: i32,
    item_start_sec: f64,
    split_at_sec: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ItemMoveParams {
    track_index: i32,
    item_start_sec: f64,
    new_start_sec: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct MarkerAddParams {
    position_sec: f64,
    name: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RegionAddParams {
    start_sec: f64,
    end_sec: f64,
    name: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ViewScrollToParams {
    position_sec: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ViewSetZoomParams {
    pixels_per_sec: f64,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ActionParams {
    command_id: i32,
    section: Option<i32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ComposeAndRenderParams {
    output_path: String,
    notes: Vec<ComposeNote>,
    track_name: Option<String>,
    overwrite: Option<bool>,
    auto_instrument: Option<bool>,
    auto_limiter: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RunReascriptParams {
    code: String,
}

/// MCP tool surface. `backend` is a `dyn DawBackend` so the DAW-agnostic
/// tools (transport/tracks/fx/render/project) work over any adapter without
/// changing this file. `reaper_extra` additionally exposes REAPER-only
/// tools (MIDI, items, markers, view, actions, compose_and_render,
/// run_reascript) that have no cross-DAW equivalent - once a second adapter
/// (e.g. Audacity) is wired in, this field needs to become adapter-specific
/// / conditionally registered rather than always present.
#[derive(Clone)]
struct DawmcpServer {
    backend: Arc<dyn DawBackend>,
    reaper_extra: Arc<ReaperBackend>,
    #[allow(dead_code)]
    tool_router: ToolRouter<DawmcpServer>,
}

#[tool_router]
impl DawmcpServer {
    fn new(backend: Arc<dyn DawBackend>, reaper_extra: Arc<ReaperBackend>) -> Self {
        Self { backend, reaper_extra, tool_router: Self::tool_router() }
    }

    #[tool(description = "Check whether the DAW and its bridge/adapter are reachable")]
    async fn daw_status(&self) -> Result<CallToolResult, McpError> {
        let reachable = self.backend.is_reachable().await;
        if !reachable {
            return ok_json(serde_json::json!({ "reachable": false }));
        }
        let info = map_err(self.backend.describe().await)?;
        ok_json(serde_json::json!({ "reachable": true, "info": info }))
    }

    #[tool(description = "Start playback")]
    async fn transport_play(&self) -> Result<CallToolResult, McpError> {
        map_err(self.backend.play().await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Stop playback")]
    async fn transport_stop(&self) -> Result<CallToolResult, McpError> {
        map_err(self.backend.stop().await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Pause playback")]
    async fn transport_pause(&self) -> Result<CallToolResult, McpError> {
        map_err(self.backend.pause().await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Start recording")]
    async fn transport_record(&self) -> Result<CallToolResult, McpError> {
        map_err(self.backend.record().await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Seek the playhead to a position in seconds")]
    async fn transport_seek(&self, Parameters(p): Parameters<SeekParams>) -> Result<CallToolResult, McpError> {
        map_err(self.backend.seek(p.position_seconds).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Set the project tempo in BPM")]
    async fn transport_set_tempo(&self, Parameters(p): Parameters<TempoParams>) -> Result<CallToolResult, McpError> {
        map_err(self.backend.set_tempo(p.bpm).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Get transport state: playing/paused/recording, position, tempo")]
    async fn transport_get_state(&self) -> Result<CallToolResult, McpError> {
        let state = map_err(self.backend.get_state().await)?;
        ok_json(state)
    }

    #[tool(description = "List all tracks")]
    async fn track_list(&self) -> Result<CallToolResult, McpError> {
        let tracks = map_err(Tracks::list(self.backend.as_ref()).await)?;
        ok_json(tracks)
    }

    #[tool(description = "Add a new track, optionally named. -1 index means master everywhere else.")]
    async fn track_add(&self, Parameters(p): Parameters<TrackAddParams>) -> Result<CallToolResult, McpError> {
        let index = map_err(Tracks::add(self.backend.as_ref(), p.name.as_deref()).await)?;
        ok_json(serde_json::json!({ "index": index }))
    }

    #[tool(description = "Remove a track by index")]
    async fn track_remove(&self, Parameters(p): Parameters<TrackIndexParams>) -> Result<CallToolResult, McpError> {
        map_err(Tracks::remove(self.backend.as_ref(), p.track_index).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Rename a track")]
    async fn track_rename(&self, Parameters(p): Parameters<TrackRenameParams>) -> Result<CallToolResult, McpError> {
        map_err(self.backend.rename(p.track_index, &p.name).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Set a track's volume in dB")]
    async fn track_set_volume_db(&self, Parameters(p): Parameters<TrackVolumeParams>) -> Result<CallToolResult, McpError> {
        map_err(self.backend.set_volume_db(p.track_index, p.db).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Set a track's pan, -1.0 (left) to 1.0 (right)")]
    async fn track_set_pan(&self, Parameters(p): Parameters<TrackPanParams>) -> Result<CallToolResult, McpError> {
        map_err(self.backend.set_pan(p.track_index, p.pan).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Mute or unmute a track")]
    async fn track_set_mute(&self, Parameters(p): Parameters<TrackMuteParams>) -> Result<CallToolResult, McpError> {
        map_err(self.backend.set_mute(p.track_index, p.mute).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Solo or unsolo a track")]
    async fn track_set_solo(&self, Parameters(p): Parameters<TrackSoloParams>) -> Result<CallToolResult, McpError> {
        map_err(self.backend.set_solo(p.track_index, p.solo).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Set a track's color (RGB 0-255 each)")]
    async fn track_set_color(&self, Parameters(p): Parameters<TrackColorParams>) -> Result<CallToolResult, McpError> {
        map_err(self.backend.set_color(p.track_index, p.r, p.g, p.b).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "List FX on a track")]
    async fn fx_list(&self, Parameters(p): Parameters<TrackIndexParams>) -> Result<CallToolResult, McpError> {
        let fx = map_err(Fx::list(self.backend.as_ref(), p.track_index).await)?;
        ok_json(fx)
    }

    #[tool(description = "Add an FX/plugin to a track by name")]
    async fn fx_add(&self, Parameters(p): Parameters<FxAddParams>) -> Result<CallToolResult, McpError> {
        let fx_index = map_err(Fx::add(self.backend.as_ref(), p.track_index, &p.fx_name).await)?;
        ok_json(serde_json::json!({ "fx_index": fx_index }))
    }

    #[tool(description = "Remove an FX from a track")]
    async fn fx_remove(&self, Parameters(p): Parameters<FxIndexParams>) -> Result<CallToolResult, McpError> {
        map_err(Fx::remove(self.backend.as_ref(), p.track_index, p.fx_index).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Enable or bypass an FX")]
    async fn fx_set_enabled(&self, Parameters(p): Parameters<FxEnabledParams>) -> Result<CallToolResult, McpError> {
        map_err(self.backend.set_enabled(p.track_index, p.fx_index, p.enabled).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Get an FX parameter's current value by index")]
    async fn fx_get_param(&self, Parameters(p): Parameters<FxParamGetParams>) -> Result<CallToolResult, McpError> {
        let value = map_err(self.backend.get_param(p.track_index, p.fx_index, p.param_index).await)?;
        ok_json(serde_json::json!({ "value": value }))
    }

    #[tool(description = "Set an FX parameter's value by index")]
    async fn fx_set_param(&self, Parameters(p): Parameters<FxParamSetParams>) -> Result<CallToolResult, McpError> {
        map_err(self.backend.set_param(p.track_index, p.fx_index, p.param_index, p.value).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Render the project (or a time range) to an audio file")]
    async fn render_project(&self, Parameters(p): Parameters<RenderParams>) -> Result<CallToolResult, McpError> {
        let output_path = map_err(
            self.backend
                .render(RenderRequest {
                    output_path: p.output_path,
                    start_seconds: p.start_seconds,
                    end_seconds: p.end_seconds,
                    overwrite: p.overwrite,
                })
                .await,
        )?;
        ok_json(serde_json::json!({ "output_path": output_path }))
    }

    #[tool(description = "Save the project")]
    async fn project_save(&self) -> Result<CallToolResult, McpError> {
        map_err(self.backend.save().await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Undo the last project action")]
    async fn project_undo(&self) -> Result<CallToolResult, McpError> {
        map_err(self.backend.undo().await)?;
        ok_json(serde_json::json!({}))
    }

    // -- REAPER-only tools below: no cross-DAW equivalent yet, see the
    // `reaper_extra` field doc comment. --

    #[tool(description = "Create an empty MIDI item on a track spanning start_sec to end_sec")]
    async fn midi_add_item(&self, Parameters(p): Parameters<MidiAddItemParams>) -> Result<CallToolResult, McpError> {
        map_err(self.reaper_extra.midi_add_item(p.track_index, p.start_sec, p.end_sec).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(
        description = "Add a MIDI note to the MIDI item on a track that starts at item_start_sec. \
                        pitch 0-127 (60 = middle C), velocity 1-127."
    )]
    async fn midi_add_note(&self, Parameters(p): Parameters<MidiAddNoteParams>) -> Result<CallToolResult, McpError> {
        map_err(
            self.reaper_extra
                .midi_add_note(
                    p.track_index,
                    p.item_start_sec,
                    p.pitch,
                    p.velocity,
                    p.note_start_sec,
                    p.note_end_sec,
                    p.channel.unwrap_or(0),
                )
                .await,
        )?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Split the media item on a track that starts at item_start_sec, at split_at_sec")]
    async fn item_split(&self, Parameters(p): Parameters<ItemSplitParams>) -> Result<CallToolResult, McpError> {
        map_err(self.reaper_extra.item_split(p.track_index, p.item_start_sec, p.split_at_sec).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Move the media item on a track that starts at item_start_sec to new_start_sec")]
    async fn item_move(&self, Parameters(p): Parameters<ItemMoveParams>) -> Result<CallToolResult, McpError> {
        map_err(self.reaper_extra.item_move(p.track_index, p.item_start_sec, p.new_start_sec).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Glue all currently selected media items into one item each (per track)")]
    async fn item_glue_selected(&self) -> Result<CallToolResult, McpError> {
        map_err(self.reaper_extra.item_glue_selected().await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Render selected items in place (applies FX/pitch/rate destructively to a new item)")]
    async fn item_render_in_place_selected(&self) -> Result<CallToolResult, McpError> {
        map_err(self.reaper_extra.item_render_in_place_selected().await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Add a project marker at the given position in seconds")]
    async fn marker_add(&self, Parameters(p): Parameters<MarkerAddParams>) -> Result<CallToolResult, McpError> {
        let index = map_err(self.reaper_extra.marker_add(p.position_sec, p.name.as_deref()).await)?;
        ok_json(serde_json::json!({ "index": index }))
    }

    #[tool(description = "Add a project region spanning start_sec to end_sec")]
    async fn region_add(&self, Parameters(p): Parameters<RegionAddParams>) -> Result<CallToolResult, McpError> {
        let index = map_err(self.reaper_extra.region_add(p.start_sec, p.end_sec, p.name.as_deref()).await)?;
        ok_json(serde_json::json!({ "index": index }))
    }

    #[tool(description = "Zoom the arrange view horizontally to fit the currently selected item(s)")]
    async fn view_zoom_to_selection(&self) -> Result<CallToolResult, McpError> {
        map_err(self.reaper_extra.view_zoom_to_selection().await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Scroll the arrange view so the given position (seconds) is visible")]
    async fn view_scroll_to(&self, Parameters(p): Parameters<ViewScrollToParams>) -> Result<CallToolResult, McpError> {
        map_err(self.reaper_extra.view_scroll_to(p.position_sec).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(description = "Set the arrange view's horizontal zoom level, in pixels per second")]
    async fn view_set_arrange_zoom(&self, Parameters(p): Parameters<ViewSetZoomParams>) -> Result<CallToolResult, McpError> {
        map_err(self.reaper_extra.view_set_arrange_zoom(p.pixels_per_sec).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(
        description = "Run a REAPER action (native or custom) by its numeric command ID. Look up \
                        command_id via REAPER's Actions list (right-click an action -> 'Copy \
                        selected action ID'). section defaults to 0 (Main)."
    )]
    async fn action_run(&self, Parameters(p): Parameters<ActionParams>) -> Result<CallToolResult, McpError> {
        map_err(self.reaper_extra.action_run(p.command_id, p.section.unwrap_or(0)).await)?;
        ok_json(serde_json::json!({}))
    }

    #[tool(
        description = "Get the on/off state of a toggle action by command ID: 1 = on, 0 = off, \
                        -1 = not a toggle action (or state unknown)"
    )]
    async fn action_get_toggle_state(&self, Parameters(p): Parameters<ActionParams>) -> Result<CallToolResult, McpError> {
        let state = map_err(self.reaper_extra.action_get_toggle_state(p.command_id, p.section.unwrap_or(0)).await)?;
        ok_json(serde_json::json!({ "state": state }))
    }

    #[tool(
        description = "One call: new track + MIDI melody from a note list + render to audio. Each \
                        note: {pitch, start_sec, end_sec, velocity?, channel?}, pitch 0-127 (60 = \
                        middle C), velocity 1-127 (default 64). auto_instrument adds ReaSynth if the \
                        new track has none (default true); auto_limiter ensures a master limiter \
                        (default true); overwrite controls whether an existing output_path is \
                        replaced or auto-renamed (default false)."
    )]
    async fn compose_and_render(&self, Parameters(p): Parameters<ComposeAndRenderParams>) -> Result<CallToolResult, McpError> {
        let result = map_err(
            self.reaper_extra
                .compose_and_render(
                    &p.output_path,
                    &p.notes,
                    p.track_name.as_deref().unwrap_or("Composed"),
                    p.overwrite.unwrap_or(false),
                    p.auto_instrument.unwrap_or(true),
                    p.auto_limiter.unwrap_or(true),
                )
                .await,
        )?;
        ok_json(result)
    }

    #[tool(
        description = "Escape hatch: execute arbitrary Lua code inside REAPER's ReaScript \
                        environment (full access to the reaper.* API) and return its result as a \
                        string. Use this for anything not covered by a dedicated tool."
    )]
    async fn run_reascript(&self, Parameters(p): Parameters<RunReascriptParams>) -> Result<CallToolResult, McpError> {
        let result = map_err(self.reaper_extra.run_reascript(&p.code).await)?;
        ok_json(serde_json::json!({ "result": result }))
    }
}

#[tool_handler]
impl ServerHandler for DawmcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
            .with_instructions(
                "Tools for controlling a live DAW instance (currently REAPER): transport, \
                 tracks, FX, and rendering. Call daw_status first if you're unsure whether \
                 the DAW and its bridge are reachable."
                    .to_string(),
            )
    }
}

/// Usage:
///   dawmcp                  # auto-install for every detected DAW, then run the MCP server over stdio
///   dawmcp --install-bridge # install/update reaper_bridge.lua, then exit (no server)
///   dawmcp --status         # print discovery/diagnostics, then exit (no server)
///   dawmcp --no-install     # run the MCP server without the auto-install step
#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--install-bridge") {
        for line in install_bridge() {
            println!("{line}");
        }
        return Ok(());
    }

    if args.iter().any(|a| a == "--status") {
        let report = run_discovery();
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    // Auto-install for every detected DAW before serving, so pointing an
    // MCP client at this binary is enough on its own - no separate manual
    // `--install-bridge`/build_and_install.bat step required. Only REAPER
    // exists today; this is the seam where an Audacity (or other DAW)
    // auto-setup call joins once that adapter lands. stderr only: stdout is
    // the MCP JSON-RPC stream once `serve(stdio())` starts below.
    if !args.iter().any(|a| a == "--no-install") {
        for line in install_bridge() {
            eprintln!("[dawmcp install] {line}");
        }
    }

    let bridge = BridgeClient::new(default_bridge_dir());
    let reaper = Arc::new(ReaperBackend::new(bridge));
    let backend: Arc<dyn DawBackend> = reaper.clone();

    let service = DawmcpServer::new(backend, reaper).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
