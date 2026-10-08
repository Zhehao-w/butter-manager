//! Local external players use a program and one game-file argument, never a shell.
use crate::domain::{Error, ExternalPlayer, Game, Result, ScanCandidate};
use crate::paths::{contained_file, working_directory};
use std::path::Path;
use std::process::Command;

pub fn qsp_config(candidate: &ScanCandidate) -> Option<ExternalPlayer> {
    candidate.qsp.as_ref().map(|qsp| ExternalPlayer {
        player_type: "QSP".into(),
        scope: "GAME_LOCAL".into(),
        game_file: if candidate.status == "ready" && qsp.game_files.len() == 1 {
            qsp.game_files.first().cloned()
        } else {
            None
        },
    })
}

/// Missing configuration is legal metadata. Supplied paths must always be local and valid.
pub fn validate(root: &Path, player: Option<&str>, config: &ExternalPlayer) -> Result<()> {
    if config.player_type != "QSP" || config.scope != "GAME_LOCAL" {
        return Err(Error::Validation(
            "目前仅支持游戏目录内的 QSP 播放器".into(),
        ));
    }
    if let Some(player) = player {
        contained_file(root, player, "exe")?;
    }
    if let Some(file) = &config.game_file {
        contained_file(root, file, "qsp")?;
    }
    Ok(())
}

pub fn command(game: &Game) -> Result<Command> {
    let root = Path::new(&game.install_path);
    let config = game
        .external_player
        .as_ref()
        .ok_or_else(|| Error::Validation("尚未配置外部播放器".into()))?;
    validate(root, game.main_executable.as_deref(), config)?;
    let player = game
        .main_executable
        .as_deref()
        .ok_or_else(|| Error::Validation("QSP 播放器未配置，请在游戏详情中选择本地 EXE".into()))?;
    let file = config
        .game_file
        .as_deref()
        .ok_or_else(|| Error::Validation("请选择 QSP 主游戏文件".into()))?;
    let mut command = Command::new(contained_file(root, player, "exe")?);
    command
        .arg(contained_file(root, file, "qsp")?)
        .current_dir(working_directory(root, &game.working_directory)?);
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_spaces_are_one_argument_and_root_cwd_without_shell_or_execution() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("魔法少女 Japanese ゲーム & [1]");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("プレイヤー 中文.exe"), b"fixture").unwrap();
        std::fs::write(root.join("彼女の冒険 游戏.qsp"), b"fixture").unwrap();
        let mut game = crate::maintenance::tests::fixture_game(&root);
        game.main_executable = Some("プレイヤー 中文.exe".into());
        game.launch_type = "EXTERNAL_PLAYER".into();
        game.external_player = Some(ExternalPlayer {
            player_type: "QSP".into(),
            scope: "GAME_LOCAL".into(),
            game_file: Some("彼女の冒険 游戏.qsp".into()),
        });
        let command = command(&game).unwrap();
        assert_eq!(
            command.get_program(),
            dunce::canonicalize(root.join("プレイヤー 中文.exe")).unwrap()
        );
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![dunce::canonicalize(root.join("彼女の冒険 游戏.qsp")).unwrap()]
        );
        assert_eq!(
            command.get_current_dir(),
            Some(dunce::canonicalize(&root).unwrap().as_path())
        );
        game.main_executable = None;
        assert!(command_for(&game).contains("播放器未配置"));
        game.main_executable = Some("../escape.exe".into());
        assert!(super::command(&game).is_err());
        game.main_executable = Some("プレイヤー 中文.exe".into());
        game.external_player.as_mut().unwrap().scope = "GLOBAL".into();
        assert!(super::command(&game).is_err());
    }
    fn command_for(game: &Game) -> String {
        super::command(game).unwrap_err().to_string()
    }
}
