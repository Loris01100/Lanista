//! Spritesheet animations: a grid of frames, played at a fixed duration per
//! frame, once or in a loop, with named markers (impact…) and an end event.

use macroquad::math::Rect;

/// A texture cut into a grid of equal frames, numbered left to right, then
/// top to bottom.
#[derive(Debug, Clone, Copy)]
pub struct SpriteSheet {
    pub columns: u16,
    pub frame_width: f32,
    pub frame_height: f32,
}

impl SpriteSheet {
    /// Source rectangle of a frame, in pixels, for `DrawTextureParams::source`.
    #[must_use]
    pub fn frame_rect(&self, frame: u16) -> Rect {
        Rect::new(
            f32::from(frame % self.columns) * self.frame_width,
            f32::from(frame / self.columns) * self.frame_height,
            self.frame_width,
            self.frame_height,
        )
    }
}

/// Consecutive frames of a spritesheet.
#[derive(Debug, Clone)]
pub struct Animation {
    pub first: u16,
    /// Number of frames, at least 1.
    pub frames: u16,
    /// Seconds per frame, positive.
    pub frame_time: f32,
    pub looping: bool,
    /// Named events fired when a frame (0-based within the animation) starts.
    pub markers: Vec<(u16, String)>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AnimationEvent<'a> {
    /// A marker's frame has just started.
    Marker(&'a str),
    /// A non-looping animation has played its last frame to the end.
    Finished,
}

/// Playback position in an animation; one per drawn entity. Start another
/// animation with a fresh `AnimationPlayer::default()`.
#[derive(Debug, Clone, Default)]
pub struct AnimationPlayer {
    /// Frames started since the beginning, the first one included.
    step: u64,
    /// Seconds spent in the current frame.
    in_frame: f32,
    started: bool,
}

impl AnimationPlayer {
    /// Advances by `dt` seconds and returns the events crossed, in order.
    ///
    /// # Panics
    /// If the animation has no frame or a non-positive frame time.
    pub fn advance<'a>(&mut self, animation: &'a Animation, dt: f32) -> Vec<AnimationEvent<'a>> {
        assert!(animation.frames > 0, "animation without frames");
        assert!(animation.frame_time > 0.0, "frame_time must be positive");
        let mut events = Vec::new();
        if !self.started {
            self.started = true;
            push_markers(animation, 0, &mut events);
        }
        self.in_frame += dt.max(0.0);
        while self.in_frame >= animation.frame_time && !self.is_finished(animation) {
            self.in_frame -= animation.frame_time;
            self.step += 1;
            if self.is_finished(animation) {
                events.push(AnimationEvent::Finished);
            } else {
                push_markers(animation, self.offset(animation), &mut events);
            }
        }
        events
    }

    /// Spritesheet frame to draw now.
    #[must_use]
    pub fn frame(&self, animation: &Animation) -> u16 {
        animation.first + self.offset(animation)
    }

    /// True once a non-looping animation is over; it stays on its last frame.
    #[must_use]
    pub fn is_finished(&self, animation: &Animation) -> bool {
        !animation.looping && self.step >= u64::from(animation.frames)
    }

    /// Current frame within the animation.
    fn offset(&self, animation: &Animation) -> u16 {
        let frames = u64::from(animation.frames);
        let offset = if animation.looping {
            self.step % frames
        } else {
            self.step.min(frames - 1)
        };
        u16::try_from(offset).expect("offset is below frames, a u16")
    }
}

fn push_markers<'a>(animation: &'a Animation, frame: u16, events: &mut Vec<AnimationEvent<'a>>) {
    events.extend(
        animation
            .markers
            .iter()
            .filter(|(at, _)| *at == frame)
            .map(|(_, name)| AnimationEvent::Marker(name)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use AnimationEvent::{Finished, Marker};

    fn attack(looping: bool) -> Animation {
        Animation {
            first: 4,
            frames: 3,
            frame_time: 0.1,
            looping,
            markers: vec![(0, "start".into()), (2, "impact".into())],
        }
    }

    #[test]
    fn frame_rect_walks_the_grid() {
        let sheet = SpriteSheet {
            columns: 4,
            frame_width: 16.0,
            frame_height: 32.0,
        };
        assert_eq!(sheet.frame_rect(0), Rect::new(0.0, 0.0, 16.0, 32.0));
        assert_eq!(sheet.frame_rect(5), Rect::new(16.0, 32.0, 16.0, 32.0));
    }

    #[test]
    fn once_fires_markers_then_finished_and_holds_last_frame() {
        let animation = attack(false);
        let mut player = AnimationPlayer::default();
        assert_eq!(player.advance(&animation, 0.0), [Marker("start")]);
        assert_eq!(player.frame(&animation), 4);
        // One big step crosses frames 1 and 2 and the end.
        assert_eq!(
            player.advance(&animation, 0.35),
            [Marker("impact"), Finished]
        );
        assert!(player.is_finished(&animation));
        assert_eq!(player.frame(&animation), 6);
        assert_eq!(player.advance(&animation, 1.0), []);
    }

    #[test]
    fn looping_wraps_and_refires_markers() {
        let animation = attack(true);
        let mut player = AnimationPlayer::default();
        player.advance(&animation, 0.25); // frame 2
        assert_eq!(player.frame(&animation), 6);
        assert_eq!(player.advance(&animation, 0.1), [Marker("start")]);
        assert_eq!(player.frame(&animation), 4);
        assert!(!player.is_finished(&animation));
    }
}
