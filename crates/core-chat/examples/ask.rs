//! Faz perguntas a uma conversa já importada (um `.db`), mostrando a ferramenta usada e o tempo.
//!
//! ```text
//! cargo run --release -p core-chat --example ask -- conversa.db "Quem responde mais rápido?"
//! cargo run --release -p core-chat --example ask -- --json conversa.db "Pergunta 1" "Pergunta 2"
//! ```

use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.first().is_some_and(|a| a == "--json");
    if json {
        args.remove(0);
    }
    let mut args = args.into_iter();
    let db_path = args.next().ok_or("uso: ask [--json] <conversa.db> <pergunta>...")?;
    let db = core_storage::Db::open(db_path)?;
    if json {
        let answers: Vec<(String, core_chat::ChatAnswer)> = args
            .map(|q| core_chat::ask(&db, &q).map(|a| (q, a)))
            .collect::<Result<_, _>>()?;
        println!("{}", serde_json::to_string_pretty(&answers)?);
        return Ok(());
    }
    for question in args {
        let t = Instant::now();
        let a = core_chat::ask(&db, &question)?;
        println!("? {question}");
        if let Some(call) = &a.tool {
            println!("  [{:?}] {}", a.route, serde_json::to_string(call)?);
        } else {
            println!("  [{:?}]", a.route);
        }
        println!("  {} ({:?})", a.text, t.elapsed());
        for c in &a.citations {
            println!(
                "    #{} {}: {}",
                c.id,
                c.author.as_deref().unwrap_or("-"),
                c.text.as_deref().unwrap_or_default()
            );
        }
    }
    Ok(())
}
