use std::sync::{Arc, RwLock};

use bevy::{prelude::ResMut, tasks::IoTaskPool};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

use crate::http;

pub struct Leaderboard {
    key: Uuid,
    leaderboard: Vec<Score>,
    updating: Arc<RwLock<Vec<Score>>>,
    host: String,
    new_player: Arc<RwLock<Option<Player>>>,
    player: Option<Player>,
}

impl Leaderboard {
    pub(crate) fn with_leaderboard(key: Uuid) -> Self {
        Self {
            key,
            leaderboard: Default::default(),
            updating: Default::default(),
            host: "https://jornet.vleue.com".to_string(),
            new_player: Default::default(),
            player: Default::default(),
        }
    }

    pub fn get_player_name(&self) -> Option<String> {
        self.player.as_ref().map(|p| p.name.clone())
    }

    pub fn create_player(&mut self, name: Option<&str>) {
        let thread_pool = IoTaskPool::get();
        let host = self.host.clone();

        let player = PlayerInput {
            name: name.map(|n| n.to_string()),
        };
        let complete_player = self.new_player.clone();

        thread_pool
            .spawn(async move {
                let player: Player =
                    http::post(&format!("{}/api/v1/players", host), &Some(player)).await;
                *complete_player.write().unwrap() = Some(player);
            })
            .detach();
    }

    pub fn send_score(&self, score: f32) -> Option<()> {
        let thread_pool = IoTaskPool::get();
        let key = self.key;
        let host = self.host.clone();

        let score_to_send = ScoreInput::new(score, self.player.as_ref().unwrap(), None);
        thread_pool
            .spawn(async move {
                http::post_and_forget(
                    &format!("{}/api/v1/scores/{}", host, key),
                    &Some(score_to_send),
                )
                .await;
            })
            .detach();

        Some(())
    }

    pub fn refresh_leaderboard(&self) {
        let thread_pool = IoTaskPool::get();
        let key = self.key;
        let host = self.host.clone();

        let leaderboard_to_update = self.updating.clone();

        thread_pool
            .spawn(async move {
                let scores = http::get(&format!("{}/api/v1/scores/{}", host, key)).await;
                *leaderboard_to_update.write().unwrap() = scores;
            })
            .detach();
    }

    pub fn get_leaderboard(&self) -> Vec<Score> {
        self.leaderboard.clone()
    }
}

pub fn done_refreshing_leaderboard(mut leaderboard: ResMut<Leaderboard>) {
    if !leaderboard
        .updating
        .try_read()
        .map(|v| v.is_empty())
        .unwrap_or(true)
    {
        let mut updated = leaderboard
            .updating
            .write()
            .unwrap()
            .drain(..)
            .collect::<Vec<_>>();
        updated.sort_unstable_by(|s1, s2| s2.score.partial_cmp(&s1.score).unwrap());
        updated.truncate(10);
        leaderboard.leaderboard = updated;
    }
    if leaderboard
        .new_player
        .try_read()
        .map(|v| v.is_some())
        .unwrap_or(false)
    {
        let new_player = leaderboard.new_player.write().unwrap().take();
        leaderboard.player = new_player;
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Score {
    pub score: f32,
    pub player: String,
    pub meta: Option<String>,
    pub timestamp: String,
}

#[derive(Serialize)]
pub struct ScoreInput {
    pub score: f32,
    pub player: Uuid,
    pub meta: Option<String>,
    pub hmac: String,
}

impl ScoreInput {
    pub fn new(score: f32, player: &Player, meta: Option<String>) -> Self {
        let mut mac = Hmac::<Sha256>::new_from_slice(player.key.as_bytes()).unwrap();
        mac.update(player.id.as_bytes());
        mac.update(&score.to_le_bytes());
        if let Some(meta) = meta.as_ref() {
            mac.update(meta.as_bytes());
        }

        let hmac = hex::encode(&mac.finalize().into_bytes()[..]);
        Self {
            score,
            player: player.id,
            meta,
            hmac,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Player {
    pub id: Uuid,
    pub key: Uuid,
    pub name: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct PlayerInput {
    pub name: Option<String>,
}
