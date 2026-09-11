use std::collections::HashMap;
use std::path::PathBuf;

use super::error::TimelineError;
use super::model::TrackKind;
use super::renderspec::RenderSpecV1;

#[derive(Debug, Clone)]
pub struct ResolvedInput {
    pub index: usize,
    pub artifact_id: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct FfmpegExecutionPlan {
    pub inputs: Vec<ResolvedInput>,
    pub filter_complex: String,
    pub args: Vec<String>,
    pub output_path: PathBuf,
    pub derived_duration_ms: i64,
}

pub struct FfmpegCompiler;

impl FfmpegCompiler {
    pub fn compile(
        spec: &RenderSpecV1,
        resolved_artifacts: &HashMap<String, PathBuf>,
        output_path: PathBuf,
    ) -> Result<FfmpegExecutionPlan, TimelineError> {
        let mut inputs: Vec<ResolvedInput> = Vec::new();
        let mut artifact_to_input_idx: HashMap<String, usize> = HashMap::new();

        // 1. Gather distinct inputs in deterministic order
        for track in &spec.tracks {
            for clip in &track.clips {
                if !artifact_to_input_idx.contains_key(&clip.artifact_id) {
                    let path = resolved_artifacts
                        .get(&clip.artifact_id)
                        .ok_or_else(|| TimelineError::ArtifactNotFound(clip.artifact_id.clone()))?;
                    let idx = inputs.len();
                    inputs.push(ResolvedInput {
                        index: idx,
                        artifact_id: clip.artifact_id.clone(),
                        path: path.clone(),
                    });
                    artifact_to_input_idx.insert(clip.artifact_id.clone(), idx);
                }
            }
        }

        // 2. Compute derived duration across all clips
        let mut max_end_ms: i64 = 0;
        for track in &spec.tracks {
            for clip in &track.clips {
                let dur = clip.source_out_ms - clip.source_in_ms;
                let end = clip.timeline_start_ms + dur;
                if end > max_end_ms {
                    max_end_ms = end;
                }
            }
        }

        if max_end_ms == 0 {
            return Err(TimelineError::RenderSpecInvalid(
                "Timeline has zero duration".to_string(),
            ));
        }

        let dur_s = max_end_ms as f64 / 1000.0;
        let width = spec.output.width;
        let height = spec.output.height;
        let fps = spec.output.frame_rate;

        let mut filter_parts: Vec<String> = Vec::new();

        // 3. Generate base black canvas
        let fps_str = if fps.fract() == 0.0 {
            format!("{}", fps as u64)
        } else {
            format!("{:.2}", fps)
        };
        filter_parts.push(format!(
            "color=c=black:s={}x{}:r={}:d={:.3}[base_v]",
            width, height, fps_str, dur_s
        ));

        // 4. Sort video tracks by order ASC (0 is bottom)
        let mut video_tracks: Vec<_> = spec
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Video && !t.muted)
            .collect();
        video_tracks.sort_by_key(|t| t.order);

        let mut current_v_label = "[base_v]".to_string();
        let mut overlay_step = 0;

        for track in video_tracks {
            // clip.muted suppresses AUDIO only; video frames must remain visible
            let mut clips: Vec<_> = track
                .clips
                .iter()
                .filter(|c| c.has_video)
                .cloned()
                .collect();
            clips.sort_by_key(|c| c.timeline_start_ms);

            for clip in clips {
                let in_idx = artifact_to_input_idx[&clip.artifact_id];
                let in_s = clip.source_in_ms as f64 / 1000.0;
                let out_s = clip.source_out_ms as f64 / 1000.0;
                let start_s = clip.timeline_start_ms as f64 / 1000.0;
                let end_s = start_s + (out_s - in_s);

                let trimmed_label = format!("[v_clip_{}_trimmed]", overlay_step);
                let scaled_label = format!("[v_clip_{}_scaled]", overlay_step);
                let next_v_label = format!("[v_layer_{}]", overlay_step);

                // Trim & Timeline PTS offset: setpts=PTS-STARTPTS+<startSeconds>/TB
                filter_parts.push(format!(
                    "[{}:v]trim=start={:.3}:end={:.3},setpts=PTS-STARTPTS+{:.3}/TB{}",
                    in_idx, in_s, out_s, start_s, trimmed_label
                ));

                // Scale to fit canvas preserving aspect ratio
                filter_parts.push(format!(
                    "{}scale=w={}:h={}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2,setsar=1{}",
                    trimmed_label, width, height, width, height, scaled_label
                ));

                // Overlay with enable between(t, start, end)
                filter_parts.push(format!(
                    "{}{}overlay=x=0:y=0:enable='between(t,{:.3},{:.3})'{}",
                    current_v_label, scaled_label, start_s, end_s, next_v_label
                ));

                current_v_label = next_v_label;
                overlay_step += 1;
            }
        }

        if overlay_step == 0 {
            filter_parts.push(format!(
                "{}[out_v]",
                current_v_label.replace("[base_v]", "[base_v]format=yuv420p")
            ));
        } else {
            filter_parts.push(format!(
                "{}[out_v]",
                current_v_label.replace(
                    &format!("[v_layer_{}]", overlay_step - 1),
                    &format!("[v_layer_{}]format=yuv420p", overlay_step - 1)
                )
            ));
        }

        // 5. Audio compilation: collect all clips with has_audio and volume > 0 and not muted
        let mut audio_streams: Vec<String> = Vec::new();
        let mut a_step = 0;

        for track in &spec.tracks {
            if track.muted {
                continue;
            }

            for clip in &track.clips {
                if clip.muted || clip.volume <= 0.0 || !clip.has_audio {
                    continue;
                }

                let in_idx = artifact_to_input_idx[&clip.artifact_id];
                let in_s = clip.source_in_ms as f64 / 1000.0;
                let out_s = clip.source_out_ms as f64 / 1000.0;
                let delay_ms = clip.timeline_start_ms;
                let a_label = format!("[a_{}]", a_step);

                filter_parts.push(format!(
                    "[{}:a]atrim=start={:.3}:end={:.3},asetpts=PTS-STARTPTS,volume={:.2},adelay={}|{}:all=1{}",
                    in_idx, in_s, out_s, clip.volume, delay_ms, delay_ms, a_label
                ));
                audio_streams.push(a_label);
                a_step += 1;
            }
        }

        if audio_streams.is_empty() {
            // Silence matching exact derived duration
            filter_parts.push(format!(
                "anullsrc=r=48000:cl=stereo,atrim=end={:.3}[out_a]",
                dur_s
            ));
        } else if audio_streams.len() == 1 {
            filter_parts.push(format!(
                "{}[out_a]",
                audio_streams[0].replace(
                    &audio_streams[0],
                    &format!(
                        "{}atrim=end={:.3},aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo",
                        audio_streams[0], dur_s
                    )
                )
            ));
        } else {
            let mix_inputs = audio_streams.concat();
            // amix duration=longest, strictly trimmed to timeline duration
            filter_parts.push(format!(
                "{}amix=inputs={}:duration=longest,atrim=end={:.3},aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts=stereo[out_a]",
                mix_inputs,
                audio_streams.len(),
                dur_s
            ));
        }

        let filter_complex = filter_parts.join(";");

        // 6. Assemble discrete arguments array
        let mut args = vec!["-y".to_string()];
        for input in &inputs {
            args.push("-i".to_string());
            args.push(input.path.to_string_lossy().to_string());
        }

        args.push("-filter_complex".to_string());
        args.push(filter_complex.clone());
        args.push("-map".to_string());
        args.push("[out_v]".to_string());
        args.push("-map".to_string());
        args.push("[out_a]".to_string());
        args.push("-c:v".to_string());
        args.push(spec.output.video_codec.clone());
        args.push("-pix_fmt".to_string());
        args.push(spec.output.pixel_format.clone());
        args.push("-r".to_string());
        args.push(fps_str);
        args.push("-c:a".to_string());
        args.push(spec.output.audio_codec.clone());
        args.push("-b:a".to_string());
        args.push("192k".to_string());
        args.push("-ar".to_string());
        args.push("48000".to_string());
        args.push(output_path.to_string_lossy().to_string());

        Ok(FfmpegExecutionPlan {
            inputs,
            filter_complex,
            args,
            output_path,
            derived_duration_ms: max_end_ms,
        })
    }
}
