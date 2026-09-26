//! Importa um export e imprime um resumo das estatísticas, com tempos de cada etapa.
//!
//! ```text
//! cargo run --release -p core-stats --example analyze -- conversa.txt
//! cargo run --release -p core-stats --example analyze -- conversa.zip --json stats.json
//! ```

use core_stats::{StatsSettings, compute_from_db};
use core_storage::{Db, ImportOptions, import_new};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let input = args
        .get(1)
        .ok_or("uso: analyze <arquivo.txt|.zip> [--json saida.json]")?;
    let json_out = args.iter().position(|a| a == "--json").and_then(|i| args.get(i + 1));
    let db_path = std::env::temp_dir().join("whats-recap-analyze.db");
    let _ = std::fs::remove_file(&db_path);

    let t = Instant::now();
    let source = core_parser::Source::open(input)?;
    let summary = import_new(
        &db_path,
        &source,
        &ImportOptions::default(),
        &mut |_| {},
        &Default::default(),
    )?;
    println!(
        "importação: {} mensagens, {} sessões em {:?}",
        summary.message_count,
        summary.session_count,
        t.elapsed()
    );

    let mut db = Db::open(&db_path)?;
    if source.has_media() {
        let t = Instant::now();
        let r = core_media::index_media(&mut db, &source, &Default::default(), &mut |_, _| {})?;
        println!(
            "mídia: {} figurinhas, {} áudios ({} ausentes, {} com falha) em {:?}",
            r.stickers,
            r.audios,
            r.missing,
            r.failed,
            t.elapsed()
        );
    }
    let t = Instant::now();
    let stats = compute_from_db(&db, &StatsSettings::default())?;
    println!("estatísticas em {:?}\n", t.elapsed());
    if let Some(path) = json_out {
        std::fs::write(path, serde_json::to_string_pretty(&stats)?)?;
        println!("JSON salvo em {path} (banco em {})\n", db_path.display());
    }

    for (i, a) in stats.authors.iter().enumerate() {
        let p = &stats.per_author[i];
        let r = &stats.response_times[i];
        println!(
            "{a}: {} mensagens, {} palavras, resposta média {:.0}s / mediana {:.0}s",
            p.messages, p.words, r.mean_secs, r.median_secs
        );
        let top: Vec<_> = stats.top_words_by_author[i]
            .iter()
            .take(5)
            .map(|c| c.item.as_str())
            .collect();
        println!("  top palavras: {}", top.join(", "));
        let much: Vec<_> = stats.exclusive_words[i]
            .much_more
            .iter()
            .take(5)
            .map(|w| format!("{} ({:.1}×)", w.word, w.ratio.unwrap_or(0.0)))
            .collect();
        println!("  usa muito mais: {}", much.join(", "));
    }
    if let Some(m) = &stats.media {
        println!(
            "\nfigurinhas distintas: {} (top: {:?})",
            m.distinct_stickers,
            m.top_stickers
                .iter()
                .take(3)
                .map(|s| (s.count, s.variants))
                .collect::<Vec<_>>()
        );
        for (i, a) in m.audio.iter().enumerate() {
            println!(
                "{}: {} áudios, {:.1} min no total, média {:.0}s",
                stats.authors[i],
                a.count,
                a.total_ms as f64 / 60_000.0,
                a.avg_ms / 1000.0
            );
        }
    }
    let emojis: Vec<_> = stats
        .top_emojis
        .iter()
        .take(5)
        .map(|c| format!("{} {}", c.item, c.count))
        .collect();
    println!("\ntop emojis: {}", emojis.join("  "));
    println!(
        "dias ativos: {} de {} ({:.1}%)\n\n{}",
        stats.days.active_days, stats.days.span_days, stats.days.active_pct, stats.weekday_chart_text
    );
    Ok(())
}
