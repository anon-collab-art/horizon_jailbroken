//! The owner gate from the outside. Upstream, neither `horizon-launcher play` nor the MCP
//! server's `play` starts the game while no account that owns Minecraft: Java Edition is in the
//! launcher. This single-player fork opens that gate (`account::allow_offline_play`), so both do
//! start — and stop at what the *local files* lack, never at Microsoft.
//! Set `HORIZON_REQUIRE_MS_ACCOUNT=1` to put the original gates back and check this build against
//! upstream behaviour; the tests below do exactly that for the refusal wording.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const REFUSED: &str = "Sign in with a Microsoft account that owns Minecraft: Java Edition to play.";
/// What `play` says once the Microsoft gate is open but the game's resources are not prepared
/// yet: it names the local folder to point at, and asks for no account.
const NOT_PREPARED: &str = "is not prepared yet";

/// Fresh homes with one installed (not prepared) version and an offline name, as a player who
/// played offline before the gate has.
fn home(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("horizon-owner-gate-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let version = root.join("launcher/versions/v2026.10.27.01");
    std::fs::create_dir_all(&version).unwrap();
    std::fs::write(version.join(".installed"), "").unwrap();
    std::fs::write(
        root.join("launcher/accounts.json"),
        r#"{"accounts":[],"selected":null,"offline":["Builder_1"],"offline_selected":"Builder_1"}"#,
    )
    .unwrap();
    root
}

fn launcher(root: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_horizon-launcher"));
    cmd.args(args)
        .env("HORIZON_HOME", root.join("game"))
        .env("HORIZON_LAUNCHER_HOME", root.join("launcher"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("HORIZON_LAUNCHER_NO_SELF_UPDATE", "1")
        .env("HORIZON_NO_AUDIO", "1")
        .env("HORIZON_TELEMETRY_URL", "off")
        // Memory only: the test never reads the player's keychain.
        .env("HORIZON_NO_KEYCHAIN", "1")
        .env("HORIZON_MSA_CLIENT_ID", "test-client");
    cmd
}

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr)
}

#[test]
fn the_cli_starts_the_game_without_a_microsoft_account() {
    let root = home("cli");
    let out = launcher(&root, &["play"]).output().unwrap();
    let said = text(&out);
    // The Microsoft gate lets the start through; the run stops on the local files instead,
    // because this fresh home has an installed version whose resources were never prepared.
    assert!(!out.status.success(), "{said}");
    assert!(said.contains(NOT_PREPARED), "{said}");
    assert!(!said.contains(REFUSED), "the sign-in gate came back: {said}");
    // Nothing of the game ran, and the offline name is still there.
    assert!(!root.join("game/sessions").exists());
    let accounts = std::fs::read_to_string(root.join("launcher/accounts.json")).unwrap();
    assert!(accounts.contains("Builder_1"), "{accounts}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_cli_refuses_to_play_when_the_gate_is_put_back() {
    // `HORIZON_REQUIRE_MS_ACCOUNT=1`: the same binary, upstream's behaviour, for comparison.
    let root = home("cli-strict");
    let out = launcher(&root, &["play"])
        .env("HORIZON_REQUIRE_MS_ACCOUNT", "1")
        .output()
        .unwrap();
    let said = text(&out);
    assert!(!out.status.success(), "{said}");
    assert!(said.contains(REFUSED), "{said}");
    assert!(said.contains("horizon-launcher account sign-in"), "{said}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_mcp_server_starts_the_game_without_a_microsoft_account() {
    let root = home("mcp");
    let mut child = launcher(&root, &["mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let requests = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"play","arguments":{}}}"#,
    ];
    let mut stdin = child.stdin.take().unwrap();
    for r in requests {
        writeln!(stdin, "{r}").unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let reply = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|v| v["id"] == 2)
        .expect("a reply to play");
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    let said = reply["result"]["content"][0]["text"].as_str().unwrap();
    // Same as the CLI: past Microsoft, stopped by the local files.
    assert!(!said.contains(REFUSED), "the sign-in gate came back: {said}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_mcp_server_refuses_to_play_when_the_gate_is_put_back() {
    let root = home("mcp-strict");
    let mut child = launcher(&root, &["mcp"])
        .env("HORIZON_REQUIRE_MS_ACCOUNT", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let requests = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"play","arguments":{}}}"#,
    ];
    let mut stdin = child.stdin.take().unwrap();
    for r in requests {
        writeln!(stdin, "{r}").unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let reply = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|v| v["id"] == 2)
        .expect("a reply to play");
    assert_eq!(reply["result"]["isError"], true, "{reply}");
    let said = reply["result"]["content"][0]["text"].as_str().unwrap();
    assert!(said.contains(REFUSED), "{said}");
    let _ = std::fs::remove_dir_all(&root);
}
