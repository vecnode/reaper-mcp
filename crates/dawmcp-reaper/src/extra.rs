//! REAPER-specific operations with no equivalent in `dawmcp_core::DawBackend`
//! (MIDI note editing, media items, markers/regions, view/zoom, native
//! action control, the `compose_and_render` composite tool, and the
//! `run_reascript` escape hatch). These are inherent methods on
//! [`ReaperBackend`], not part of the DAW-agnostic trait, mirroring how the
//! original Python `tools/midi.py` etc. were REAPER-only modules alongside
//! the shared ones. `dawmcp-server` calls these directly against the
//! concrete `ReaperBackend`, not through `dyn DawBackend`.

use dawmcp_core::DawResult;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::backend::ReaperBackend;

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ComposeNote {
    pub pitch: i32,
    pub start_sec: f64,
    pub end_sec: f64,
    pub velocity: Option<i32>,
    pub channel: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComposeResult {
    pub track_index: i32,
    pub render_end_sec: f64,
    pub output_path: Option<String>,
}

impl ReaperBackend {
    pub async fn midi_add_item(&self, track_index: i32, start_sec: f64, end_sec: f64) -> DawResult<()> {
        self.bridge()
            .call(
                "midi_add_item",
                json!({ "track_index": track_index, "start_sec": start_sec, "end_sec": end_sec }),
            )
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("midi_add_item", e))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn midi_add_note(
        &self,
        track_index: i32,
        item_start_sec: f64,
        pitch: i32,
        velocity: i32,
        note_start_sec: f64,
        note_end_sec: f64,
        channel: i32,
    ) -> DawResult<()> {
        self.bridge()
            .call(
                "midi_add_note",
                json!({
                    "track_index": track_index,
                    "item_start_sec": item_start_sec,
                    "pitch": pitch,
                    "velocity": velocity,
                    "note_start_sec": note_start_sec,
                    "note_end_sec": note_end_sec,
                    "channel": channel,
                }),
            )
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("midi_add_note", e))
    }

    /// No dedicated `ops.item_split` handler exists in the Lua bridge - this
    /// goes through `run_reascript`, same as the Python `item_split` tool did.
    pub async fn item_split(&self, track_index: i32, item_start_sec: f64, split_at_sec: f64) -> DawResult<()> {
        let code = format!(
            r#"
local tr = reaper.GetTrack(0, {track_index})
for i = 0, reaper.CountTrackMediaItems(tr) - 1 do
  local it = reaper.GetTrackMediaItem(tr, i)
  if math.abs(reaper.GetMediaItemInfo_Value(it, "D_POSITION") - ({item_start_sec})) < 0.001 then
    reaper.SplitMediaItem(it, {split_at_sec})
    return "ok"
  end
end
error("no item found starting at {item_start_sec} on track {track_index}")
"#
        );
        self.run_reascript(&code).await.map(|_| ())
    }

    pub async fn item_move(&self, track_index: i32, item_start_sec: f64, new_start_sec: f64) -> DawResult<()> {
        let code = format!(
            r#"
local tr = reaper.GetTrack(0, {track_index})
for i = 0, reaper.CountTrackMediaItems(tr) - 1 do
  local it = reaper.GetTrackMediaItem(tr, i)
  if math.abs(reaper.GetMediaItemInfo_Value(it, "D_POSITION") - ({item_start_sec})) < 0.001 then
    reaper.SetMediaItemInfo_Value(it, "D_POSITION", {new_start_sec})
    return "ok"
  end
end
error("no item found starting at {item_start_sec} on track {track_index}")
"#
        );
        self.run_reascript(&code).await.map(|_| ())
    }

    pub async fn item_glue_selected(&self) -> DawResult<()> {
        self.run_reascript("reaper.Main_OnCommand(41588, 0)\nreturn \"ok\"\n")
            .await
            .map(|_| ())
    }

    pub async fn item_render_in_place_selected(&self) -> DawResult<()> {
        self.run_reascript("reaper.Main_OnCommand(41999, 0)\nreturn \"ok\"\n")
            .await
            .map(|_| ())
    }

    pub async fn marker_add(&self, position_sec: f64, name: Option<&str>) -> DawResult<i32> {
        let result = self
            .bridge()
            .call("marker_add", json!({ "position_sec": position_sec, "name": name.unwrap_or("") }))
            .await
            .map_err(|e| Self::map_err("marker_add", e))?;
        Ok(result.get("index").and_then(|v| v.as_i64()).unwrap_or(0) as i32)
    }

    pub async fn region_add(&self, start_sec: f64, end_sec: f64, name: Option<&str>) -> DawResult<i32> {
        let result = self
            .bridge()
            .call(
                "region_add",
                json!({ "start_sec": start_sec, "end_sec": end_sec, "name": name.unwrap_or("") }),
            )
            .await
            .map_err(|e| Self::map_err("region_add", e))?;
        Ok(result.get("index").and_then(|v| v.as_i64()).unwrap_or(0) as i32)
    }

    pub async fn view_zoom_to_selection(&self) -> DawResult<()> {
        self.bridge()
            .call("view_zoom_to_selection", json!({}))
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("view_zoom_to_selection", e))
    }

    pub async fn view_scroll_to(&self, position_sec: f64) -> DawResult<()> {
        self.bridge()
            .call("view_scroll_to", json!({ "position_sec": position_sec }))
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("view_scroll_to", e))
    }

    pub async fn view_set_arrange_zoom(&self, pixels_per_sec: f64) -> DawResult<()> {
        self.bridge()
            .call("view_set_arrange_zoom", json!({ "pixels_per_sec": pixels_per_sec }))
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("view_set_arrange_zoom", e))
    }

    pub async fn action_run(&self, command_id: i32, section: i32) -> DawResult<()> {
        self.bridge()
            .call("action_run", json!({ "command_id": command_id, "section": section }))
            .await
            .map(|_| ())
            .map_err(|e| Self::map_err("action_run", e))
    }

    pub async fn action_get_toggle_state(&self, command_id: i32, section: i32) -> DawResult<i32> {
        let result = self
            .bridge()
            .call("action_get_state", json!({ "command_id": command_id, "section": section }))
            .await
            .map_err(|e| Self::map_err("action_get_state", e))?;
        Ok(result.get("state").and_then(|v| v.as_i64()).unwrap_or(-1) as i32)
    }

    /// One call: new track + MIDI notes + render to audio. 60s timeout,
    /// matching the Python tool - a render can take longer than the default
    /// 5s bridge timeout.
    pub async fn compose_and_render(
        &self,
        output_path: &str,
        notes: &[ComposeNote],
        track_name: &str,
        overwrite: bool,
        auto_instrument: bool,
        auto_limiter: bool,
    ) -> DawResult<ComposeResult> {
        let result = self
            .bridge()
            .call_with_timeout(
                "compose_and_render",
                json!({
                    "output_path": output_path,
                    "notes": notes,
                    "track_name": track_name,
                    "overwrite": overwrite,
                    "auto_instrument": auto_instrument,
                    "auto_limiter": auto_limiter,
                }),
                std::time::Duration::from_secs(60),
            )
            .await
            .map_err(|e| Self::map_err("compose_and_render", e))?;
        Ok(ComposeResult {
            track_index: result.get("track_index").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            render_end_sec: result.get("render_end_sec").and_then(|v| v.as_f64()).unwrap_or(0.0),
            output_path: result.get("output_path").and_then(|v| v.as_str()).map(str::to_string),
        })
    }

    /// Escape hatch: execute arbitrary Lua inside REAPER's ReaScript
    /// environment, for anything not covered by a dedicated method.
    pub async fn run_reascript(&self, code: &str) -> DawResult<String> {
        let result = self
            .bridge()
            .call("run_reascript", json!({ "code": code }))
            .await
            .map_err(|e| Self::map_err("run_reascript", e))?;
        Ok(result.get("result").and_then(|v| v.as_str()).unwrap_or("").to_string())
    }
}
