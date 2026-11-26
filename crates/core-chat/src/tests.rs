use super::*;
use serde_json::json;
use std::cell::RefCell;
use std::sync::atomic::AtomicBool;

// Valores esperados, contados à mão:
// - "amor": Ana 1, Bruno 3 ("amooor" conta); "te amo": Ana 2 em 2024, Bruno 1 em 2025.
// - Madrugada (0h–5h): Ana 2, Bruno 1.
// - Janeiro de 2024: 6 mensagens (o mês com mais); 2024 tem 10 meses vazios (fevereiro e abril–dezembro).
// - Respostas (gap de 6h): Ana 60s, 600s, 240s (mediana 240s); Bruno 120s, 60s (mediana 90s).
const CHAT: &str = "\
01/01/2024 01:00 - Ana Souza: amor, você acordou?
01/01/2024 01:02 - Bruno: acordei amor
01/01/2024 01:03 - Ana Souza: te amo
01/01/2024 14:00 - Bruno: amooor vamos almoçar em Floripa?
01/01/2024 14:10 - Ana Souza: bora
02/01/2024 23:00 - Bruno: boa noite amor
15/03/2024 10:00 - Ana Souza: saudade da viagem
15/03/2024 10:01 - Bruno: <Mídia oculta>
15/03/2024 10:05 - Ana Souza: te amo muito
10/02/2025 09:00 - Bruno: te amo
";

fn with_db(f: impl FnOnce(&Db)) {
    with_chat(CHAT, |db| f(db));
}

fn with_chat(chat: &str, f: impl FnOnce(&mut Db)) {
    let dir = std::env::temp_dir().join(format!(
        "wr-chat-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let txt = dir.join("chat.txt");
    std::fs::write(&txt, chat).unwrap();
    let db_path = dir.join("chat.db");
    let source = core_parser::Source::open(&txt).unwrap();
    core_storage::import_new(
        &db_path,
        &source,
        &Default::default(),
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    f(&mut Db::open(&db_path).unwrap());
    std::fs::remove_dir_all(dir).ok();
}

fn ask_ok(db: &Db, q: &str) -> ChatAnswer {
    let a = ask(db, q, None).unwrap();
    println!("{q}\n  → {}", a.text);
    a
}

#[test]
fn tools_directly() {
    with_db(|db| {
        let c = Conversation::open(db).unwrap();
        let r = c
            .execute(&ToolCall::CountWord {
                word: "amor".into(),
                author: None,
                from: None,
                to: None,
            })
            .unwrap();
        let ToolResult::CountWord {
            total,
            by_author,
            messages,
            examples,
            ..
        } = r
        else {
            panic!()
        };
        assert_eq!((total, by_author, messages), (4, vec![1, 3], 4));
        assert_eq!(examples, vec![1, 2, 4, 6]);

        let r = c
            .execute(&ToolCall::MessagesByPeriod {
                granularity: Granularity::Month,
                author: None,
                from: None,
                to: None,
            })
            .unwrap();
        let ToolResult::MessagesByPeriod {
            periods, top, bottom, ..
        } = r
        else {
            panic!()
        };
        assert_eq!(periods.len(), 14, "jan/2024 a fev/2025, com zeros");
        assert_eq!((top[0].period.as_str(), top[0].total), ("2024-01", 6));
        assert_eq!((bottom[0].period.as_str(), bottom[0].total), ("2024-02", 0));

        let r = c
            .execute(&ToolCall::MessagesByPeriod {
                granularity: Granularity::Weekday,
                author: None,
                from: None,
                to: None,
            })
            .unwrap();
        let ToolResult::MessagesByPeriod { periods, .. } = r else {
            panic!()
        };
        // Segundas-feiras (0): 01/01/2024 (5 mensagens) e 10/02/2025 (1).
        assert_eq!(periods.iter().find(|p| p.period == "0").unwrap().total, 6);

        let r = c
            .execute(&ToolCall::MessagesByPeriod {
                granularity: Granularity::Week,
                author: None,
                from: None,
                to: None,
            })
            .unwrap();
        let ToolResult::MessagesByPeriod { top, .. } = r else {
            panic!()
        };
        assert_eq!((top[0].period.as_str(), top[0].total), ("2024-01-01", 6));

        let r = c
            .execute(&ToolCall::ResponseTime {
                author: None,
                from: None,
                to: None,
            })
            .unwrap();
        let ToolResult::ResponseTime { by_author, fastest } = r else {
            panic!()
        };
        assert_eq!((by_author[0].count, by_author[0].median_secs), (3, 240.0));
        assert_eq!((by_author[1].count, by_author[1].median_secs), (2, 90.0));
        assert_eq!(fastest, Some(1));

        let r = c
            .execute(&ToolCall::FirstOccurrence {
                word: "te amo".into(),
                author: Some("bruno".into()),
            })
            .unwrap();
        let ToolResult::FirstOccurrence { message: Some(m), .. } = r else {
            panic!()
        };
        assert_eq!(m.id, 10);

        assert!(matches!(
            c.execute(&ToolCall::MessagesByHour {
                author: Some("Carla".into()),
                from: None,
                to: None
            }),
            Err(ToolError::UnknownAuthor { .. })
        ));
    });
}

#[test]
fn list_messages() {
    with_db(|db| {
        let c = Conversation::open(db).unwrap();
        // Das 23h às 2h: 01:00, 01:02 e 01:03 de 01/01 e 23:00 de 02/01.
        let night = ListFilter {
            hour_from: Some(23),
            hour_to: Some(2),
            ..Default::default()
        };
        let r = c.execute(&ToolCall::ListMessages(night.clone())).unwrap();
        assert_eq!(
            r,
            ToolResult::ListMessages {
                total: 4,
                by_author: vec![2, 2],
                examples: vec![1, 2, 3, 6]
            }
        );
        let ids =
            |f: &ListFilter, limit| -> Vec<i64> { c.list_messages(f, limit).unwrap().iter().map(|m| m.id).collect() };
        assert_eq!(ids(&night, 2), [1, 2]);
        let te_amo = ListFilter {
            author: Some("ana".into()),
            word: Some("te amo".into()),
            ..Default::default()
        };
        assert_eq!(ids(&te_amo, 100), [3, 9]);
        // A mídia oculta entra na lista (não tem texto, mas é mensagem).
        let day = ListFilter {
            author: Some("Bruno".into()),
            from: NaiveDate::from_ymd_opt(2024, 3, 15),
            to: NaiveDate::from_ymd_opt(2024, 3, 15),
            ..Default::default()
        };
        assert_eq!(ids(&day, 100), [8]);
        assert!(matches!(
            c.execute(&ToolCall::ListMessages(ListFilter {
                hour_from: Some(25),
                ..Default::default()
            })),
            Err(ToolError::InvalidHour(25))
        ));

        let a = ask_ok(db, "quais foram as mensagens da Ana de madrugada?");
        assert_eq!(a.text, "Ana Souza mandou 2 mensagens da meia-noite às 6h.");
        assert_eq!(a.citations.iter().map(|m| m.id).collect::<Vec<_>>(), [1, 3]);
        let a = ask_ok(db, "quais foram as mensagens entre 23h e 2h?");
        assert_eq!(a.text, "Foram 4 mensagens das 23h às 2h (Ana Souza: 2 · Bruno: 2).");
        let a = ask_ok(db, "mostra as mensagens com 'amor' de 2025");
        assert_eq!(a.text, "Não encontrei mensagens com “amor” em 2025.");
    });
}

#[test]
fn who_asks() {
    with_db(|db| {
        assert_eq!(me(db).unwrap(), None, "sem \"Você\" no export");
        let a = ask_ok(db, "quantas vezes ela falou 'amor'?");
        assert_eq!(a.tool.unwrap().author(), None);

        set_me(db, Some("Bruno")).unwrap();
        assert_eq!(me(db).unwrap().as_deref(), Some("Bruno"));
        let a = ask_ok(db, "quantas vezes ela falou 'amor'?");
        assert_eq!(a.tool.unwrap().author(), Some("Ana Souza"));
        assert!(a.text.starts_with("Ana Souza escreveu “amor” 1 vez"), "{}", a.text);
        let a = ask_ok(db, "quantas vezes eu falei 'amor'?");
        assert_eq!(a.tool.unwrap().author(), Some("Bruno"));

        assert!(matches!(
            set_me(db, Some("Carla")),
            Err(ToolError::UnknownAuthor { .. })
        ));
        set_me(db, None).unwrap();
        assert_eq!(me(db).unwrap(), None);
    });
}

#[test]
fn questions_end_to_end() {
    with_db(|db| {
        let a = ask_ok(db, "Quantas vezes o Bruno falou 'amor'?");
        assert_eq!(a.text, "Bruno escreveu “amor” 3 vezes, em 3 mensagens.");
        assert_eq!(a.citations.len(), 3);
        assert_eq!(a.interpreted_by, Some(Interpreter::Heuristic));

        let a = ask_ok(db, "quantas vezes a Ana disse te amo em 2024?");
        assert_eq!(a.text, "Ana Souza escreveu “te amo” 2 vezes em 2024, em 2 mensagens.");

        let a = ask_ok(db, "quem fala mais 'amor'?");
        assert_eq!(
            a.text,
            "“amor” aparece 4 vezes (Ana Souza: 1 · Bruno: 3). Bruno usa mais."
        );

        let a = ask_ok(db, "Quem manda mais mensagem de madrugada?");
        assert_eq!(
            a.text,
            "De madrugada (0h–5h), Ana Souza manda mais mensagens: 2 contra 1 de Bruno."
        );

        let a = ask_ok(db, "Qual mês teve mais mensagens?");
        assert_eq!(a.text, "O mês com mais mensagens foi janeiro de 2024, com 6 mensagens.");

        let a = ask_ok(db, "qual mês teve menos mensagens em 2024?");
        assert_eq!(
            a.text,
            "10 meses em 2024 não tiveram nenhuma mensagem; o primeiro foi fevereiro de 2024."
        );
        let a = ask_ok(db, "qual ano teve menos mensagens?");
        assert_eq!(a.text, "O ano com menos mensagens foi 2025, com 1 mensagem.");

        let a = ask_ok(db, "Quem responde mais rápido?");
        assert_eq!(
            a.text,
            "Bruno responde mais rápido: mediana de 1min 30s, contra 4min de Ana Souza. (Médias: 1min 30s e 5min.)"
        );

        let a = ask_ok(db, "quanto tempo a Ana demora para responder?");
        assert_eq!(
            a.text,
            "Ana Souza leva em média 5min para responder (mediana de 4min, em 3 respostas)."
        );

        let a = ask_ok(db, "Quando foi a primeira vez que alguém disse \"te amo\"?");
        assert_eq!(
            a.text,
            "A primeira vez foi em 01/01/2024 às 01:03, por Ana Souza: “te amo”"
        );
        assert_eq!(a.citations[0].id, 3);

        let a = ask_ok(db, "que horas o Bruno mais manda mensagem?");
        assert!(
            a.text.starts_with("O horário com mais mensagens de Bruno é das "),
            "{}",
            a.text
        );

        let a = ask_ok(db, "quantas mensagens foram em março de 2024?");
        assert_eq!(a.text, "Foram 3 mensagens em março de 2024 (Ana Souza: 2 · Bruno: 1).");
    });
}

#[test]
fn other_routes() {
    with_db(|db| {
        let a = ask_ok(db, "como evoluiu a nossa relação?");
        assert_eq!(a.route, Route::Global);
        assert!(a.tool.is_none());

        let a = ask_ok(db, "o que a gente falou sobre a viagem para Floripa?");
        assert_eq!(a.route, Route::Retrieval);
        let ids: Vec<i64> = a.citations.iter().map(|m| m.id).collect();
        assert!(ids.contains(&4) && ids.contains(&7), "{ids:?}");

        let a = ask_ok(db, "oi, tudo bem?");
        assert_eq!(a.route, Route::Undecided);
        assert!(a.text.contains("Experimente"));

        let a = ask_ok(db, "quantas vezes a gente brigou?");
        assert_eq!(a.route, Route::Quantitative);
        assert!(a.tool.is_none());
    });
}

#[test]
fn examples_use_names() {
    let ex = examples(&["Ana Souza".into(), "Bruno".into()]);
    assert_eq!(ex[0], "Quantas vezes Bruno falou \"amor\"?");
    assert!(ex[4].contains("Ana disse"));
}

/// LLM de respostas fixas: uma para o roteador e outra para a escolha de ferramenta.
struct FakeLlm {
    route: Result<Value, String>,
    tool: Result<Value, String>,
    /// (prompt de sistema, pergunta, schema) de cada chamada.
    calls: RefCell<Vec<(String, String, Value)>>,
}

impl FakeLlm {
    fn new(route: &str, tool: Value) -> Self {
        FakeLlm {
            route: Ok(json!({ "type": route })),
            tool: Ok(tool),
            calls: RefCell::default(),
        }
    }
}

impl Llm for FakeLlm {
    fn complete(&self, system: &str, user: &str, schema: &Value, _: u32) -> Result<Value, String> {
        self.calls
            .borrow_mut()
            .push((system.to_string(), user.to_string(), schema.clone()));
        if system == llm::router_prompt() {
            self.route.clone()
        } else {
            self.tool.clone()
        }
    }
}

#[test]
fn llm_answers_what_the_rules_do_not_understand() {
    with_db(|db| {
        // O modelo preenche o período inteiro da conversa: equivale a não filtrar.
        let fake = FakeLlm::new(
            "quantitative",
            json!({ "tool": "messages_by_hour", "args": { "from": "2024-01-01", "to": "2025-02-10" } }),
        );
        let a = ask(db, "e aí, a gente conversa muito?", Some(&fake)).unwrap();
        assert_eq!(a.route, Route::Quantitative);
        assert_eq!(a.interpreted_by, Some(Interpreter::Llm));
        assert_eq!(
            a.tool,
            Some(ToolCall::MessagesByHour {
                author: None,
                from: None,
                to: None
            })
        );
        assert!(a.text.starts_with("Foram 10 mensagens"), "{}", a.text);

        let calls = fake.calls.borrow();
        assert_eq!(calls.len(), 2, "roteador e ferramenta");
        let (system, user, schema) = &calls[1];
        assert_eq!(user, "e aí, a gente conversa muito?");
        assert!(
            system.contains("entre Ana Souza e Bruno (de 2024-01-01 a 2025-02-10)"),
            "{system}"
        );
        assert_eq!(schema, &llm::tool_schema(&["Ana Souza".into(), "Bruno".into()]));
    });
}

#[test]
fn forgotten_relative_period_is_filled_in() {
    with_db(|db| {
        let today = NaiveDate::from_ymd_opt(2025, 3, 1).unwrap();
        let fake = FakeLlm::new(
            "quantitative",
            json!({ "tool": "response_time", "args": { "author": "Bruno" } }),
        );
        let a = ask_on(
            db,
            "quanto o bruno demora pra responder no ano passado?",
            Some(&fake),
            today,
        )
        .unwrap();
        assert_eq!(a.interpreted_by, Some(Interpreter::Llm));
        // A conversa começa em 2024-01-01: o início do ano é redundante.
        assert_eq!(a.tool.unwrap().range(), (None, NaiveDate::from_ymd_opt(2024, 12, 31)));
        // Com as datas do modelo, nada muda.
        let fake = FakeLlm::new(
            "quantitative",
            json!({ "tool": "list_messages", "args": { "author": "Ana Souza", "from": "2024-12-24", "to": "2024-12-25" } }),
        );
        let a = ask_on(
            db,
            "mostra o que a Ana mandou no natal do ano passado",
            Some(&fake),
            today,
        )
        .unwrap();
        assert_eq!(a.route, Route::Quantitative);
        assert_eq!(
            a.tool.unwrap().range(),
            (
                NaiveDate::from_ymd_opt(2024, 12, 24),
                NaiveDate::from_ymd_opt(2024, 12, 25)
            )
        );
        assert_eq!(fake.calls.borrow().len(), 1, "o roteador por regras já decidiu");
    });
}

#[test]
fn rules_come_before_the_llm() {
    with_db(|db| {
        let fake = FakeLlm::new("global", json!({}));
        let a = ask(db, "Quantas vezes o Bruno falou 'amor'?", Some(&fake)).unwrap();
        assert_eq!(a.interpreted_by, Some(Interpreter::Heuristic));
        assert!(fake.calls.borrow().is_empty());
    });
}

#[test]
fn llm_without_a_fitting_tool() {
    with_db(|db| {
        let fake = FakeLlm::new("quantitative", json!({ "tool": "none", "args": {} }));
        let a = ask(db, "quantas vezes a gente brigou?", Some(&fake)).unwrap();
        assert!(a.tool.is_none());
        assert!(a.text.contains("não dá para responder com as contagens"), "{}", a.text);
        // O roteador por regras já decidiu (contagem): só a ferramenta passa pelo modelo.
        assert_eq!(fake.calls.borrow().len(), 1);
    });
}

#[test]
fn llm_routes_undecided_questions() {
    with_db(|db| {
        let fake = FakeLlm::new("global", json!({}));
        let a = ask(db, "oi, tudo bem?", Some(&fake)).unwrap();
        assert_eq!(a.route, Route::Global);
        assert!(a.tool.is_none());
    });
}

#[test]
fn llm_failure_falls_back_to_the_rules() {
    with_db(|db| {
        let fake = FakeLlm {
            route: Err("o servidor caiu".into()),
            tool: Err("o servidor caiu".into()),
            calls: RefCell::default(),
        };
        let a = ask(db, "oi, tudo bem?", Some(&fake)).unwrap();
        assert!(a.tool.is_none());
        assert!(
            a.text.contains("o servidor caiu") && a.text.contains("Experimente"),
            "{}",
            a.text
        );
    });
}

#[test]
fn intent_follows_the_question() {
    use intent::{Intent, intent_for};
    let by_hour = ToolCall::MessagesByHour {
        author: None,
        from: None,
        to: None,
    };
    assert_eq!(
        intent_for(&by_hour, "que horas a gente mais conversa?"),
        Intent::PeakHour
    );
    assert!(matches!(
        intent_for(&by_hour, "quem manda menos mensagem de manhã?"),
        Intent::WhoMost {
            most: false,
            hours: Some((6, 11, _))
        }
    ));
    let period = ToolCall::MessagesByPeriod {
        granularity: Granularity::Month,
        author: None,
        from: None,
        to: None,
    };
    assert_eq!(
        intent_for(&period, "mês com menos conversa"),
        Intent::TopPeriod { most: false }
    );
}

#[test]
fn compares_parts_of_the_day() {
    with_db(|db| {
        // Dia (6h–17h): 6 mensagens; noite (18h–23h): 1.
        let a = ask_ok(db, "A gente conversa mais de noite ou de dia?");
        assert_eq!(a.interpreted_by, Some(Interpreter::Heuristic));
        assert_eq!(
            a.text,
            "Vocês conversam mais de dia (6h–17h): 6 mensagens, contra 1 de noite (18h–23h). Por hora, a média é de 0,2 de noite e 0,5 de dia."
        );
        let a = ask_ok(db, "o Bruno manda menos mensagem de madrugada ou de manhã?");
        assert_eq!(
            a.text,
            "Bruno manda menos mensagens de madrugada (0h–5h): 1 mensagem, contra 2 de manhã (6h–11h)."
        );

        // Escolhida pelo modelo: a resposta compara as partes que a pergunta cita.
        let fake = FakeLlm::new("quantitative", json!({ "tool": "messages_by_hour", "args": {} }));
        let a = ask(db, "é mais de madrugada, de manhã ou de tarde?", Some(&fake)).unwrap();
        // Faixas do mesmo tamanho: sem a média por hora.
        assert_eq!(
            a.text,
            "Vocês conversam mais de manhã (6h–11h): 4 mensagens, contra 3 de madrugada (0h–5h) e 2 de tarde (12h–17h)."
        );
    });
}

#[test]
fn parts_of_the_day_in_the_question() {
    use intent::{DAYTIME, PARTS_OF_DAY, parts_of_day};
    let names = |q: &str| parts_of_day(q).iter().map(|p| p.2).collect::<Vec<_>>();
    assert_eq!(names("a gente conversa mais de noite ou de dia?"), ["noite", "dia"]);
    assert_eq!(names("durante o dia ou de madrugada"), ["dia", "madrugada"]);
    assert_eq!(names("qual dia da semana tem mais mensagens?"), Vec::<&str>::new());
    assert_eq!(names("o que ela mandou de dia 15 a 20?"), Vec::<&str>::new());
    assert_eq!(parts_of_day("de manha")[0], PARTS_OF_DAY[1]);
    assert_eq!(parts_of_day("quantas mensagens de dia")[0], DAYTIME);
}

#[test]
fn warmup_prompts_are_the_ones_the_questions_use() {
    with_db(|db| {
        let prompts = llm_prompts(db).unwrap();
        let fake = FakeLlm::new("quantitative", json!({ "tool": "messages_by_hour", "args": {} }));
        ask(db, "oi, tudo bem?", Some(&fake)).unwrap();
        let calls = fake.calls.borrow();
        assert_eq!(calls.len(), 2, "roteador e ferramenta");
        for ((system, _, schema), (p_system, p_schema)) in calls.iter().zip(&prompts) {
            assert_eq!(system, p_system);
            assert_eq!(schema, p_schema);
        }
    });
}

/// Embedder de teste: saco de palavras (minúsculas, sem acentos) espalhado por hash em 512
/// dimensões. Textos com palavras em comum ficam parecidos.
struct FakeEmbedder {
    id: &'static str,
    calls: RefCell<usize>,
    /// Falha a partir desta chamada (para simular o servidor caindo no meio).
    fail_from: Option<usize>,
}

impl FakeEmbedder {
    fn new() -> Self {
        FakeEmbedder {
            id: "fake",
            calls: RefCell::default(),
            fail_from: None,
        }
    }
}

impl rag::Embedder for FakeEmbedder {
    fn id(&self) -> &str {
        self.id
    }

    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        let n = {
            let mut c = self.calls.borrow_mut();
            *c += 1;
            *c
        };
        if self.fail_from.is_some_and(|f| n >= f) {
            return Err("servidor fora do ar".into());
        }
        Ok(texts
            .iter()
            .map(|t| {
                let mut v = vec![0f32; 512];
                for w in router::fold(t)
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| w.len() > 2)
                {
                    let h = w.bytes().fold(7u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64));
                    v[(h % 512) as usize] += 1.0;
                }
                let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
                v.iter().map(|x| x / norm).collect()
            })
            .collect())
    }
}

fn index_all(db: &mut Db, e: &FakeEmbedder) -> rag::IndexStatus {
    rag::index(db, e, &AtomicBool::new(false), &mut |_, _| {}).unwrap()
}

#[test]
fn chunks_follow_pauses() {
    with_db(|db| {
        // 01:00–01:03; 14:00–14:10; 23:00; 15/03 (a mídia #8 fica dentro, mas não entra no texto); 2025.
        assert_eq!(
            rag::plan_chunks(db).unwrap(),
            [(1, 3), (4, 5), (6, 6), (7, 9), (10, 10)]
        );
    });
}

#[test]
fn long_talks_become_overlapping_windows() {
    let chat: String = (0..30)
        .map(|i| {
            format!(
                "01/01/2024 10:{i:02} - {}: mensagem {i}\n",
                if i % 2 == 0 { "Ana" } else { "Bruno" }
            )
        })
        .collect();
    with_chat(&chat, |db| {
        assert_eq!(rag::plan_chunks(db).unwrap(), [(1, 12), (11, 22), (21, 30)]);
    });
}

#[test]
fn index_resumes_and_rebuilds_for_another_embedder() {
    // 12 dias de 30 mensagens: 36 trechos, mais que um lote (32).
    let chat: String = (0..360)
        .map(|i| format!("{:02}/01/2024 10:{:02} - Ana: assunto {i}\n", 1 + i / 30, i % 30))
        .collect();
    with_chat(&chat, |db| {
        let e = FakeEmbedder::new();
        assert_eq!(rag::status(db, "fake").unwrap().total, 0);
        // Cai na segunda chamada: o primeiro lote fica gravado.
        let failing = FakeEmbedder {
            fail_from: Some(2),
            ..FakeEmbedder::new()
        };
        assert!(rag::index(db, &failing, &AtomicBool::new(false), &mut |_, _| {}).is_err());
        let st = rag::status(db, "fake").unwrap();
        assert!(st.embedded > 0 && st.embedded < st.total && !st.ready, "{st:?}");
        let first_batch = st.embedded;

        // Retoma: só os trechos que faltavam vão ao embedder.
        let mut seen = Vec::new();
        let st = rag::index(db, &e, &AtomicBool::new(false), &mut |done, total| {
            seen.push((done, total))
        })
        .unwrap();
        assert!(st.ready, "{st:?}");
        assert_eq!(seen.first().unwrap().0, first_batch);
        assert_eq!(*seen.last().unwrap(), (st.total, st.total));

        // Outro embedder: o índice é refeito (cancelado antes do primeiro lote, fica vazio).
        let other = FakeEmbedder {
            id: "outro",
            ..FakeEmbedder::new()
        };
        assert!(rag::status(db, "outro").unwrap().stale);
        let st = rag::index(db, &other, &AtomicBool::new(true), &mut |_, _| {}).unwrap();
        assert_eq!((st.embedded, st.stale, st.ready), (0, false, false));
        assert!(index_all(db, &other).ready);
    });
}

#[test]
fn search_finds_the_matching_chunk() {
    with_chat(CHAT, |db| {
        let e = FakeEmbedder::new();
        index_all(db, &e);
        let hits = rag::search(db, &e, "a gente teve saudade da viagem?", 2).unwrap();
        assert_eq!((hits[0].chunk.start_id, hits[0].chunk.end_id), (7, 9));
        assert!(hits[0].score >= hits[1].score);
    });
}

#[test]
fn content_questions_answer_with_checked_citations() {
    with_chat(CHAT, |db| {
        let e = FakeEmbedder::new();
        let fake = FakeLlm::new(
            "retrieval",
            json!({ "found": true, "answer": "Ela disse que sentia saudade da viagem (#7).", "citations": [7, 999] }),
        );
        let q = "o que ela disse sobre a viagem?";
        let today = NaiveDate::from_ymd_opt(2025, 3, 1).unwrap();

        // Sem índice: avisa e sugere mensagens com as palavras.
        let a = ask_with(db, q, Some(&fake), Some(&e), today).unwrap();
        assert!(a.text.contains("precisa ser indexada"), "{}", a.text);
        assert!(fake.calls.borrow().is_empty());
        // Sem embedder: precisa dos modelos.
        let a = ask_with(db, q, Some(&fake), None, today).unwrap();
        assert!(a.text.contains("busca por significado"), "{}", a.text);

        index_all(db, &e);
        let a = ask_with(db, q, Some(&fake), Some(&e), today).unwrap();
        assert_eq!(a.route, Route::Retrieval);
        assert_eq!(a.interpreted_by, Some(Interpreter::Llm));
        assert_eq!(a.text, "Ela disse que sentia saudade da viagem (#7).");
        // O id inventado (999) não vira citação.
        assert_eq!(a.citations.iter().map(|m| m.id).collect::<Vec<_>>(), [7]);
        let calls = fake.calls.borrow();
        assert_eq!(calls.len(), 1, "o roteador por regras já decidiu");
        assert!(
            calls[0].0.contains("[7] 2024-03-15 Ana Souza: saudade da viagem"),
            "{}",
            calls[0].0
        );
        assert_eq!(calls[0].2, llm::rag_schema());
    });
}

#[test]
fn content_answers_without_valid_citations_are_refused() {
    with_chat(CHAT, |db| {
        let e = FakeEmbedder::new();
        index_all(db, &e);
        let today = NaiveDate::from_ymd_opt(2025, 3, 1).unwrap();
        let q = "o que ela disse sobre a viagem?";
        let fake = FakeLlm::new(
            "retrieval",
            json!({ "found": true, "answer": "Foi em Paris (#999).", "citations": [999] }),
        );
        let a = ask_with(db, q, Some(&fake), Some(&e), today).unwrap();
        assert!(a.text.starts_with("Não achei uma mensagem que confirme"), "{}", a.text);
        assert!(a.citations.is_empty());

        let fake = FakeLlm::new(
            "retrieval",
            json!({ "found": false, "answer": "Não encontrei isso na conversa.", "citations": [] }),
        );
        let a = ask_with(db, q, Some(&fake), Some(&e), today).unwrap();
        assert_eq!(a.text, "Não encontrei isso na conversa.");

        // O modelo fora do ar vira resposta, não erro.
        let fake = FakeLlm {
            tool: Err("conexão recusada".into()),
            ..FakeLlm::new("retrieval", json!({}))
        };
        let a = ask_with(db, q, Some(&fake), Some(&e), today).unwrap();
        assert!(a.text.contains("conexão recusada"), "{}", a.text);
    });
}

#[test]
fn followups_continue_the_previous_content_question() {
    with_chat(CHAT, |db| {
        let e = FakeEmbedder::new();
        index_all(db, &e);
        let today = NaiveDate::from_ymd_opt(2025, 3, 1).unwrap();
        let previous = PreviousTurn {
            question: "o que ela disse sobre a viagem?".into(),
            route: Route::Retrieval,
            answer: "Ela disse que sentia saudade da viagem (#7).".into(),
            citations: vec![7],
        };
        let fake = FakeLlm::new(
            "quantitative",
            json!({ "found": true, "answer": "Depois ela disse que o amava (#9).", "citations": [9] }),
        );
        let a = ask_after(db, "e depois?", Some(&previous), Some(&fake), Some(&e), today).unwrap();
        assert_eq!((a.route, a.followup), (Route::Retrieval, true));
        assert_eq!(a.citations.iter().map(|m| m.id).collect::<Vec<_>>(), [9]);
        let calls = fake.calls.borrow();
        assert_eq!(calls.len(), 1, "sem passar pelo roteador");
        let (system, user, _) = &calls[0];
        assert!(
            user.starts_with("Pergunta anterior: o que ela disse sobre a viagem?"),
            "{user}"
        );
        assert!(user.ends_with("e depois?"), "{user}");
        // As mensagens depois da citação anterior entram no contexto.
        assert!(system.contains("[9] 2024-03-15 Ana Souza: te amo muito"), "{system}");
        drop(calls);

        // Depois de uma contagem, "e depois?" não é continuação de conteúdo: vai para o roteador.
        let counted = PreviousTurn {
            route: Route::Quantitative,
            ..previous.clone()
        };
        let fake = FakeLlm::new("global", json!({}));
        let a = ask_after(db, "e depois?", Some(&counted), Some(&fake), Some(&e), today).unwrap();
        assert!(!a.followup);
        assert_eq!(fake.calls.borrow()[0].0, llm::router_prompt());

        // Uma pergunta nova e completa não é continuação.
        let fake = FakeLlm::new(
            "retrieval",
            json!({ "found": false, "answer": "Não encontrei isso na conversa.", "citations": [] }),
        );
        let a = ask_after(
            db,
            "o que a gente falou sobre Floripa?",
            Some(&previous),
            Some(&fake),
            Some(&e),
            today,
        )
        .unwrap();
        assert!(!a.followup);
        assert!(!fake.calls.borrow()[0].1.contains("Pergunta anterior"));
    });
}
