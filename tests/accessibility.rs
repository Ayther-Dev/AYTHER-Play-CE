//! Play se puede usar sin ver la pantalla, y se puede leer con poca vista (#663).
//!
//! Dos cosas distintas que este archivo ata, porque fallan igual de silenciosas:
//!
//! * **Lo que un lector de pantalla oye.** Antes de #663 el `.slint` no declaraba
//!   ni una propiedad `accessible-*` en 6800 líneas: para un lector de pantalla
//!   Play era una imagen, con los textos existiendo como píxeles y ninguna
//!   superficie diciendo qué era ni qué hacía.
//! * **El contraste de los tokens.** El encabezado de `tokens.slint` afirmaba
//!   «contraste text-dim/bg-window ≈ 4.6:1, AA a ese tamaño». Medido daba
//!   **4.46**, que no llega al 4.5 que pide WCAG 2.2 AA para texto normal — y
//!   contra `bg-panel` daba 3.98. La afirmación estaba escrita a mano y nadie la
//!   había calculado.
//!
//! Como el resto de los oráculos de Play, esto no abre una ventana: lee el
//! `.slint` como texto, que es lo único que corre en el CI de Linux sin display.

use std::path::PathBuf;

fn play_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
fn read(rel: &str) -> String {
    let p = play_dir().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("no se pudo leer {}: {e}", p.display()))
}

// ---- Lo que se anuncia ---------------------------------------------------

/// Devuelve el cuerpo de un bloque `{ … }` a partir de un índice cualquiera.
fn body(s: &str, since: usize) -> &str {
    let b = s.as_bytes();
    let opening_brace = since + s[since..].find('{').expect("el bloque no abre llave");
    let mut level = 0usize;
    for i in opening_brace..b.len() {
        match b[i] {
            b'{' => level += 1,
            b'}' => {
                level -= 1;
                if level == 0 {
                    return &s[opening_brace..=i];
                }
            }
            _ => {}
        }
    }
    panic!("el bloque que empieza en {since} no cierra");
}

/// El cuerpo del bloque que CONTIENE al índice dado: se retrocede until la
/// llave abierta que todavía no cerró.
fn parent_block(s: &str, since: usize) -> &str {
    let b = s.as_bytes();
    let mut level = 0i32;
    let mut i = since;
    while i > 0 {
        i -= 1;
        match b[i] {
            b'}' => level += 1,
            b'{' => {
                if level == 0 {
                    return body(s, i);
                }
                level -= 1;
            }
            _ => {}
        }
    }
    &s[..0]
}

/// Un componente del `.slint`: su nombre y su cuerpo.
fn components(ui: &str) -> Vec<(String, &str)> {
    let mut out = Vec::new();
    for (i, _) in ui.match_indices("\ncomponent ") {
        let start = i + 1;
        let header = &ui[start..start + ui[start..].find('{').unwrap_or(0)];
        let name = header.split_whitespace().nth(1).unwrap_or("?").to_string();
        out.push((name, body(ui, start)));
    }
    out
}

/// El rol declarado por un cuerpo de componente, si declara alguno.
fn role(c: &str) -> Option<&str> {
    let i = c.find("accessible-role:")?;
    let remaining = &c[i + "accessible-role:".len()..];
    let end = remaining.find(';')?;
    Some(
        remaining[..end]
            .trim()
            .trim_start_matches("AccessibleRole."),
    )
}

/// Todo componente que se toca, se anuncia.
///
/// EL DEFECTO (#663): un `TouchArea` es lo que hace que algo se pueda usar con
/// el mouse, y no dice absolutamente nada a un lector de pantalla. Antes de esta
/// issue había doce componentes con `TouchArea` y cero roles declarados: la
/// biblioteca entera, las filas de Ajustes, los botones y los diálogos eran
/// rectángulos mudos.
///
/// Este oráculo no puede probar que la etiqueta sea BUENA — eso se revisa
/// leyendo. Lo que cierra es que no vuelva a haber superficies sin ninguna.
#[test]
fn every_touchable_component_announces_itself() {
    let ui = read("ui/appwindow.slint");
    let mut inspected_count = 0usize;

    for (name, c) in components(&ui) {
        if !c.contains("TouchArea") {
            continue;
        }
        inspected_count += 1;
        let r = role(c).unwrap_or_else(|| {
            panic!(
                "el componente `{name}` tiene un `TouchArea` y no declara `accessible-role`.\n\
                 Sin rol, Slint NO expone ninguna de las otras propiedades accesibles: el\n\
                 componente queda mudo para un lector de pantalla. Si de verdad no es un\n\
                 control —un velo que se come los clics, por ejemplo— declaralo\n\
                 `accessible-role: none;` con el porqué, para que se lea como una decisión y\n\
                 no como un olvido (#663)."
            )
        });

        // `none` es una respuesta válida y explícita: no es un control.
        if r == "none" {
            continue;
        }

        assert!(
            c.contains("accessible-label:"),
            "`{name}` declara el rol `{r}` y no una etiqueta. Un control anunciado sin \
             nombre se oye como «botón», y en una pantalla con nueve botones eso no alcanza \
             para elegir ninguno."
        );

        // Un control que se puede accionar tiene que poder accionarse DESDE el
        // lector: sin `accessible-action-default` se anuncia y no se aprieta.
        if matches!(r, "button" | "checkbox" | "list-item") {
            assert!(
                c.contains("accessible-action-default"),
                "`{name}` se anuncia como `{r}` pero no declara \
                 `accessible-action-default`: un lector de pantalla lo lee y no lo puede \
                 activar, que es la mitad de la accesibilidad que falta."
            );
        }
    }

    // No-vacuidad: si el buscador de componentes deja de encontrarlos, el bucle
    // no comprueba nada y el test pasa en verde para siempre.
    assert!(
        inspected_count >= 10,
        "sólo encontré {inspected_count} componentes con `TouchArea`. O el `.slint` cambió de forma \
         y esto dejó de medir lo que dice medir, o el buscador se rompió: en los dos casos \
         hay que arreglarlo y no borrarlo."
    );
}

/// Y sabe reconocer el defecto: un componente que se toca y no se anuncia.
#[test]
fn the_oracle_sees_a_mute_component() {
    let silent_component = "\ncomponent Button inherits Rectangle {\n TouchArea { }\n}\n";
    let comps = components(silent_component);
    assert_eq!(
        comps.len(),
        1,
        "el buscador no encontró el componente de prueba"
    );
    assert_eq!(comps[0].0, "Button");
    assert!(
        role(comps[0].1).is_none(),
        "un componente sin rol tiene que salir sin rol"
    );

    let accessible_component = "\ncomponent Button inherits Rectangle {\n accessible-role: button;\n \
                accessible-label: \"x\";\n TouchArea { }\n}\n";
    assert_eq!(role(components(accessible_component)[0].1), Some("button"));

    // Y que el rol cualificado del enum se lea igual que el corto: la tira de
    // pestañas usa `AccessibleRole.tab` porque la variable del `for` se llama
    // `tab`, y un oráculo que no lo contemple diría que no hay rol.
    let qualified_role = "\ncomponent T inherits Rectangle {\n accessible-role: AccessibleRole.tab;\n \
                TouchArea { }\n}\n";
    assert_eq!(role(components(qualified_role)[0].1), Some("tab"));
}

/// Y los controles dibujados a mano dentro de `AppWindow` también.
///
/// El oráculo de arriba mira COMPONENTES, que es donde las instancias se
/// multiplican —cada tarjeta, cada fila de Ajustes, cada botón—. Pero la ventana
/// dibuja además una veintena de controles sueltos: los botones de cada diálogo,
/// el par de vista grilla/lista, la equis que descarta la actualización. Un
/// `TouchArea` con nombre es un control con nombre, y varios de ésos son
/// **sólo un ícono**: sin etiqueta, un lector de pantalla no tiene absolutamente
/// nada que decir de ellos.
#[test]
fn loose_window_controls_announce_themselves_too() {
    let ui = read("ui/appwindow.slint");
    let start = ui
        .find(
            "
export component AppWindow",
        )
        .expect("no está AppWindow");
    let window = &ui[start..];

    let mut inspected_count = 0usize;
    let mut silent_components: Vec<String> = Vec::new();
    for (i, _) in window.match_indices(":= TouchArea {") {
        // El nombre es lo que va antes del `:=` en la misma línea.
        let line = window[..i].rfind('\n').map_or(0, |n| n + 1);
        let name = window[line..i]
            .trim()
            .rsplit(' ')
            .next()
            .unwrap_or("?")
            .to_string();
        inspected_count += 1;
        // El rol puede estar en el `TouchArea` o en el elemento que lo
        // CONTIENE, y las dos son correctas según qué sea el control: la
        // pestaña es el rectángulo entero —su `TouchArea` es sólo cómo se
        // toca—, mientras que un botón dibujado a mano es el `TouchArea`
        // mismo. Mirar sólo uno de los dos convertiría a este oráculo en una
        // regla de estilo en vez de una de accesibilidad.
        let c = body(window, i);
        let parent = parent_block(window, i);
        let declared_role = role(c).or_else(|| role(parent));
        if declared_role.is_none() {
            silent_components.push(format!(
                "{name} (línea ~{})",
                ui[..start + i].matches('\n').count() + 1
            ));
        } else {
            let location = if role(c).is_some() { c } else { parent };
            assert!(
                location.contains("accessible-label:"),
                "el control `{name}` declara rol y no etiqueta"
            );
            assert!(
                location.contains("accessible-action-default"),
                "el control `{name}` se anuncia y no se puede activar desde el lector"
            );
        }
    }

    assert!(
        inspected_count >= 15,
        "sólo {inspected_count} controles con nombre: el lector se rompió"
    );
    assert!(
        silent_components.is_empty(),
        "hay controles dibujados a mano sin rol accesible:
  {}

Un `TouchArea` con          name es un control. Si es icónico, la label es LO ÚNICO que lo nombra (#663).",
        silent_components.join("
  ")
    );
}

// ---- Lo que se ve --------------------------------------------------------

fn rgb(h: &str) -> (f64, f64, f64) {
    let v = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap() as f64;
    (v(0), v(2), v(4))
}

fn luminance((r, g, b): (f64, f64, f64)) -> f64 {
    let ch = |v: f64| {
        let v = v / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * ch(r) + 0.7152 * ch(g) + 0.0722 * ch(b)
}

/// El contraste de WCAG 2.x entre dos colores opacos.
fn contrast(a: (f64, f64, f64), b: (f64, f64, f64)) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Los tokens de color de `tokens.slint`, sin los que llevan alpha: un color
/// translúcido no tiene contraste propio —depende de sobre qué se apoye— y
/// meterlo en la tabla sería medir una cosa que no existe.
fn opaque_tokens(src: &str) -> Vec<(String, (f64, f64, f64))> {
    let mut out = Vec::new();
    for line in src.lines() {
        let Some(i) = line.find("out property <color>") else {
            continue;
        };
        let remaining = &line[i + "out property <color>".len()..];
        let Some((name, val)) = remaining.split_once(':') else {
            continue;
        };
        let val = val.trim();
        let Some(hex) = val.strip_prefix('#') else {
            continue;
        };
        let hex: String = hex.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        if hex.len() != 6 {
            continue; // 8 dígitos = alpha
        }
        out.push((name.trim().to_string(), rgb(&hex)));
    }
    out
}

/// El texto de Play cumple WCAG 2.2 AA sobre cualquiera de sus fondos.
///
/// **Por qué se mide contra los CINCO fondos y no contra el que cada color usa
/// hoy:** un token existe justamente para poder ponerse en otro lado mañana. Uno
/// que sólo cumple donde está puesto es una trampa con fecha.
///
/// El umbral es 4.5:1, el de texto normal. Play podría reclamar 3:1 para lo que
/// dibuja a 24 px o más, pero su token más usado —`text-body-sm`, 18 px— es
/// texto normal, y sostener dos umbrales según dónde se use cada color es cómo
/// se pierde la cuenta.
#[test]
fn token_contrast_meets_aa() {
    const BACKGROUNDS: [&str; 5] = [
        "bg-window",
        "bg-panel",
        "bg-elevated",
        "bg-input",
        "bg-well",
    ];
    const MIN_TEXT_CONTRAST: f64 = 4.5;

    // Lo que TODAVÍA no cumple, con su issue. No es una lista de perdón: es lo
    // que hace que el oráculo pueda existir hoy en vez de dentro de dos issues,
    // y que cualquier violación NUEVA falle. Los dos que quedan no se arreglan
    // subiendo la luminosidad —`danger` a 4.5:1 deja de leerse como peligro, y
    // `border` a 3:1 cambia el borde de cada panel de la aplicación—, así que
    // son decisiones de diseño y viven en su propia issue.
    const PENDING_CONTRAST_PAIRS: [(&str, &str); 2] = [
        ("danger", "#666: el rojo de peligro no llega ni a 3:1"),
        ("border", "#666: el borde no llega a 3:1 (SC 1.4.11)"),
    ];
    // `brand` es el logotipo, y WCAG exime a los logotipos del contraste
    // (SC 1.4.3 y 1.4.11): es la marca, no información.
    const EXEMPT_TOKENS: [&str; 1] = ["brand"];

    let src = read("ui/tokens.slint");
    let tokens = opaque_tokens(&src);
    assert!(
        tokens.len() >= 15,
        "`tokens.slint` declara sólo {} colores opacos: el lector se rompió",
        tokens.len()
    );

    let background = |n: &str| {
        tokens
            .iter()
            .find(|(k, _)| k == n)
            .unwrap_or_else(|| panic!("`tokens.slint` no declara el fondo `{n}`"))
            .1
    };

    let mut measured_count = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for (name, color) in &tokens {
        if BACKGROUNDS.contains(&name.as_str())
            || EXEMPT_TOKENS.contains(&name.as_str())
            || name.starts_with("overlay-")
            || name == "modal-dim"
            || name == "border-soft"   // separador decorativo, no delimita un control
            || name == "accent-fg"
        // va SOBRE el acento, no sobre un fondo
        {
            continue;
        }
        let pending_pair = PENDING_CONTRAST_PAIRS.iter().find(|(n, _)| n == name);
        for f in BACKGROUNDS {
            let r = contrast(*color, background(f));
            measured_count += 1;
            if r < MIN_TEXT_CONTRAST && pending_pair.is_none() {
                failures.push(format!(
                    "{name} sobre {f}: {r:.2} (hace falta {MIN_TEXT_CONTRAST})"
                ));
            }
        }
        // Y al revés: un pendiente que ya cumple tiene que salir de la lista, o
        // la lista se vuelve permanente por inercia.
        if let Some((_, note)) = pending_pair {
            let worst_ratio = BACKGROUNDS
                .iter()
                .map(|f| contrast(*color, background(f)))
                .fold(f64::MAX, f64::min);
            assert!(
                worst_ratio < MIN_TEXT_CONTRAST,
                "`{name}` ya cumple AA en todos los fondos (peor caso {worst_ratio:.2}): sacalo de \
                 PENDIENTES y cerrá la issue que lo justifica ({note})."
            );
        }
    }

    assert!(
        measured_count >= 40,
        "se midieron sólo {measured_count} pares: el bucle no está recorriendo nada"
    );
    assert!(
        failures.is_empty(),
        "hay texto que no cumple WCAG 2.2 AA (4.5:1):\n  {}\n\nLos colores salen de \
         `play-ce.pen` y `tokens.slint` se GENERA: el arreglo va en el .pen y después \
         `py play-ce/tools/gen_tokens.py`. Subir la luminosidad dejando el tono suele alcanzar \
         — así se arreglaron `text-dim`, `info`, `warning` y `accent-dim` en #663.",
        failures.join("\n  ")
    );
}

/// Lo que el encabezado de `tokens.slint` AFIRMA sobre el contraste, es cierto.
///
/// El encabezado decía «contraste text-dim/bg-window ≈ 4.6:1, AA a ese tamaño»
/// y el valor real era 4.46 — por debajo del 4.5 que la propia frase invocaba.
/// Estaba escrito a mano, nadie lo calculó, y se leía como una garantía. Un
/// número afirmado en un comentario es una promesa; ésta la verifica.
#[test]
fn the_tokens_header_does_not_promise_contrast_it_does_not_deliver() {
    let src = read("ui/tokens.slint");
    let tokens = opaque_tokens(&src);
    let get = |n: &str| tokens.iter().find(|(k, _)| k == n).unwrap().1;
    let real = contrast(get("text-dim"), get("bg-window"));

    let i = src
        .find("contraste text-dim/bg-window ≈ ")
        .expect("el encabezado de tokens.slint dejó de afirmar el contraste de `text-dim`");
    let remaining = &src[i + "contraste text-dim/bg-window ≈ ".len()..];
    let documented_ratio: f64 = remaining
        [..remaining.find(':').expect("la afirmación no tiene «N:1»")]
        .trim()
        .parse()
        .expect("lo afirmado no es un número");

    assert!(
        (documented_ratio - real).abs() < 0.1,
        "el encabezado de `tokens.slint` afirma {documented_ratio:.2}:1 y el valor real es \
         {real:.2}:1. El encabezado lo escribe `play-ce/tools/gen_tokens.py`: corregilo ahí."
    );
    assert!(
        real >= 4.5,
        "`text-dim` sobre `bg-window` da {real:.2}:1 y el encabezado dice que es AA. Para \
         texto normal AA son 4.5:1."
    );
}
