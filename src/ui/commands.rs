use super::*;
use crate::data::{iso_date, Rule};
use ironcalc::base::{
    cf_types::{CfRuleInput, ValueOperator},
    expressions::utils::{column_to_number, number_to_column},
    types::{Color, Dxf, Fill, Link},
};
pub const FUNCTIONS: &str =
    "SUM SUMIF SUMIFS SUMPRODUCT SUBTOTAL AVERAGE AVERAGEIF AVERAGEIFS MIN MAX COUNT COUNTA COUNTBLANK COUNTIF COUNTIFS IF IFERROR IFNA IFS AND OR NOT ABS ROUND ROUNDUP ROUNDDOWN INT MOD POWER SQRT LEFT RIGHT MID LEN TRIM UPPER LOWER CONCATENATE CONCAT TEXTJOIN TEXT VALUE SUBSTITUTE VLOOKUP HLOOKUP XLOOKUP INDEX MATCH DATE TODAY NOW YEAR MONTH DAY EDATE EOMONTH NETWORKDAYS ISBLANK ISNUMBER ISTEXT ISERROR PMT PV FV NPV IRR";
fn split(s: &str) -> (&str, &str) {
    s.split_once(char::is_whitespace)
        .map(|(x, y)| (x, y.trim_start()))
        .unwrap_or((s, ""))
}
fn style(b: &mut Book, s: u32, a: Range, key: &str, v: &str) -> Result {
    b.m.update_range_style(&a.area(s), key, v)
}
fn amount(s: &str, max: i32) -> Result<i32> {
    s.parse::<i32>()
        .ok()
        .filter(|n| (1..=max).contains(n))
        .ok_or("Dimensão inválida".into())
}
impl App {
    pub fn command(&mut self, line: &str) -> Result {
        let (op, arg) = split(line);
        let (s, r, c, a) = (self.sheet, self.v.r, self.v.c, self.selection());
        let id = self.book.ws(s).sheet_id;
        match op {
            "ajuda" => self.mode = Mode::Help,
            "tema" => self.theme = !self.theme,
            "editar" => return self.begin(self.book.raw(s, r, c)),
            "ir" => self.jump(parse_range(arg)?),
            "proxima-aba" => self.adjacent(1)?,
            "aba-anterior" => self.adjacent(-1)?,
            "abrir" | "sair" if self.book.dirty() => self.mode = Mode::Confirm(op.into()),
            "sair" => self.quit = true,
            "abrir" => self.files(false)?,
            "salvar" | "copia" => {
                if self.book.readonly {
                    return Err("Somente leitura".into());
                }
                if op == "copia" || self.book.path.is_none() || self.book.copy_only {
                    self.files(true)?
                } else {
                    let p = self.book.path.clone().unwrap();
                    storage::save(&mut self.book, &p, self.backup)?;
                    self.status = format!("Salvo em {}", p.display());
                    self.mode = Mode::Nav
                }
            }
            "desfazer" | "refazer" => {
                self.book.history(op == "refazer")?;
                self.repair_sheet(id)
            }
            "copiar" | "recortar" => {
                if op == "recortar" && self.book.readonly {
                    return Err("Somente leitura".into());
                }
                self.clip = Some(self.book.clipboard(s, a, op == "recortar")?);
                self.status = format!("Bloco {} no clipboard interno", a.address());
                if self.osc52 {
                    crate::terminal::copy_osc52(std::io::stdout(), &self.clip.as_ref().unwrap().tsv)
                        .map_err(|e| e.to_string())?
                }
            }
            "colar" => {
                let p = self.clip.as_ref().ok_or("Clipboard interno vazio")?;
                self.book
                    .change(|b| b.paste(p, s, r, c, if arg.is_empty() { "normal" } else { arg }))?;
                if p.cut {
                    self.clip = None
                }
                self.status = "Colado em todas as linhas do bloco, inclusive ocultas".into()
            }
            "recalcular" => {
                self.book.finish();
                self.status = "Cálculo atualizado".into()
            }
            "importar" => {
                if self.book.readonly {
                    return Err("Somente leitura".into());
                }
                let (sep, rest) = split(arg);
                let (encoding, path) = split(rest);
                let sep = crate::interchange::delimiter(sep)?;
                let input = crate::interchange::read(std::path::Path::new(path), encoding)?;
                self.book
                    .change(|b| crate::interchange::paste(b, s, a.r, a.c, &input, sep))?;
                self.status="Importado na aba atual, inclusive nas linhas ocultas. Texto delimitado contém apenas dados.".into();
            }
            "exportar" => {
                let (scope, rest) = split(arg);
                let (sep, rest) = split(rest);
                let (encoding, path) = split(rest);
                let area = match scope {
                    "seleção" => a,
                    "aba" => {
                        let dim = self.book.ws(s).dimension();
                        Range {
                            r: 1,
                            c: 1,
                            end: dim.max_row.max(1),
                            right: dim.max_column.max(1),
                        }
                    }
                    _ => return Err("Escolha exportar seleção ou aba".into()),
                };
                crate::interchange::export(
                    &self.book,
                    s,
                    area,
                    std::path::Path::new(path),
                    crate::interchange::delimiter(sep)?,
                    encoding,
                )?;
                self.status="Valores exportados. CSV/TSV contém uma região, sem fórmulas, estilos ou outras abas.".into();
            }
            "autoajustar" => self.autofit(c)?,
            "ler-nota" => {
                self.status = self
                    .book
                    .ws(s)
                    .comments
                    .iter()
                    .find(|n| n.cell_ref == addr(r, c))
                    .map_or_else(|| "Sem nota".into(), |n| n.text.clone())
            }
            "nomes" => {
                self.status = self
                    .book
                    .m
                    .get_defined_name_list()
                    .into_iter()
                    .map(|(n, scope, f)| {
                        format!(
                            "{n}({})={f}",
                            scope.map_or("pasta".into(), |s| format!("aba {}", s + 1))
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("  ")
            }
            "buscar" => {
                let (kind, query) = split(arg);
                if query.is_empty() {
                    return Err("Use buscar valor|original|formula texto".into());
                }
                let mut hits = vec![];
                for p in self.book.m.get_model().get_all_cells() {
                    if p.index == s {
                        let raw = self.book.raw(s, p.row, p.column);
                        let v = match kind {
                            "valor" => self.book.val(s, p.row, p.column),
                            "original" => raw,
                            "formula" if raw.starts_with('=') => raw,
                            "formula" => continue,
                            _ => return Err("Escolha valor, original ou formula".into()),
                        };
                        if v.to_lowercase().contains(&query.to_lowercase()) {
                            hits.push((p.row, p.column))
                        }
                    }
                }
                hits.sort();
                let hit = hits
                    .iter()
                    .find(|p| **p > (r, c))
                    .or(hits.first())
                    .ok_or("Nenhuma ocorrência")?;
                self.focus(hit.0, hit.1, false);
                self.status = format!("{} ocorrência(s) na aba", hits.len())
            }
            _ => {
                if !matches!(
                    op,
                    "inserir-linhas"
                        | "inserir-colunas"
                        | "excluir-linhas"
                        | "excluir-colunas"
                        | "ocultar-linhas"
                        | "mostrar-linhas"
                        | "ocultar-colunas"
                        | "mostrar-colunas"
                ) {
                    a.bounded()?
                }
                self.book.change(|book| edit(book, s, (r, c), a, op, arg))?;
                if op == "nova-aba" || op == "duplicar-aba" {
                    let props = self.book.m.get_worksheets_properties();
                    let index = if op == "nova-aba" {
                        props.len() - 1
                    } else {
                        (s as usize + 1).min(props.len() - 1)
                    };
                    self.switch(index as u32)?
                } else {
                    self.repair_sheet(id)
                }
                self.status = format!("{} • {}", op, a.address())
            }
        }
        Ok(())
    }
}

fn edit(b: &mut Book, s: u32, (r, c): (i32, i32), a: Range, op: &str, arg: &str) -> Result {
    let id = b.ws(s).sheet_id;
    let area = a.area(s);
    match op {
        "nova-aba" => b.m.new_sheet(),
        "renomear-aba" => b.rename_tab(s, arg),
        "ocultar-aba" | "excluir-aba"
            if b.m
                .get_worksheets_properties()
                .iter()
                .filter(|p| p.state == "visible")
                .count()
                < 2 =>
        {
            Err("Mantenha uma aba visível".into())
        }
        "duplicar-aba" => b.duplicate_tab(s),
        "excluir-aba" => b.delete_tab(s),
        "ocultar-aba" => b.m.hide_sheet(s),
        "mostrar-aba" => {
            let idx =
                b.m.get_worksheets_properties()
                    .iter()
                    .position(|w| w.name == arg)
                    .ok_or("Aba desconhecida")?;
            b.m.unhide_sheet(idx as u32)
        }
        "mover-aba" => b.m.move_sheet(
            s,
            amount(arg, b.m.get_worksheets_properties().len() as i32)? as u32 - 1,
        ),
        "inserir-linhas" | "excluir-linhas" | "inserir-colunas" | "excluir-colunas" => {
            b.shift(s, a, op.ends_with("linhas"), op.starts_with("excluir"))
        }
        "ocultar-linhas" | "mostrar-linhas" => {
            if b.extra.filters.iter().any(|f| f.sheet == id) {
                return Err("Limpe o filtro antes de mudar a visibilidade manual".into());
            }
            if area.height > 100000 {
                return Err("Limite de 100 mil linhas por operação".into());
            }
            b.m.set_rows_hidden(s, a.r, a.end, op == "ocultar-linhas")
        }
        "ocultar-colunas" | "mostrar-colunas" => {
            b.m.set_columns_hidden(s, a.c, a.right, op == "ocultar-colunas")
        }
        "largura" => b.m.set_columns_width(
            s,
            a.c,
            a.right,
            f64::from(amount(arg, 200)?) * ironcalc::base::COLUMN_WIDTH_FACTOR,
        ),
        "altura" => {
            b.m.set_rows_height(s, a.r, a.end, f64::from(amount(arg, 40)?) * 25.0)
        }
        "congelar" | "descongelar" => {
            let (r, c) = if op == "congelar" {
                (r - 1, c - 1)
            } else {
                (0, 0)
            };
            b.m.set_frozen_rows_count(s, r)?;
            b.m.set_frozen_columns_count(s, c)
        }
        "limpar" | "limpar-tudo" | "baixo" | "direita" | "mesclar"
            if b.extra.region_managed(id, a) =>
        {
            Err("Operação atinge tabela, referência estruturada ou validação".into())
        }
        "limpar" => b.m.range_clear_contents(&area),
        "limpar-formato" => b.m.range_clear_formatting(&area),
        "limpar-tudo" => {
            b.clear_notes(s, a)?;
            b.m.range_clear_all(&area)
        }
        "baixo" => b.m.auto_fill_rows(&Range { end: a.r, ..a }.area(s), a.end),
        "direita" => {
            b.m.auto_fill_columns(&Range { right: a.c, ..a }.area(s), a.right)
        }
        "preencher" => {
            for rr in a.r..=a.end {
                for cc in a.c..=a.right {
                    b.set(s, rr, cc, arg)?
                }
            }
            Ok(())
        }
        "serie" => {
            let passo: f64 = arg
                .replace(',', ".")
                .parse()
                .map_err(|_| "Informe o incremento")?;
            let first = b
                .number(s, a.r, a.c)
                .ok_or("Primeira célula deve conter número ou data")?;
            let st = b.m.get_cell_style(s, a.r, a.c)?;
            for rr in a.r..=a.end {
                for cc in a.c..=a.right {
                    let n = first + passo * f64::from((rr - a.r) * area.width + cc - a.c);
                    let v = n
                        .to_string()
                        .replace('.', if b.m.get_locale() == "pt" { "," } else { "." });
                    b.set(s, rr, cc, &v)?
                }
            }
            style(b, s, a, "num_fmt", &st.num_fmt)
        }
        "substituir" => {
            let (scope, pair) = split(arg);
            let (old, new) = pair.split_once(" => ").ok_or("Use escopo antigo => novo")?;
            if old.is_empty() || !matches!(scope, "seleção" | "aba" | "pasta") {
                return Err("Escopo ou busca inválida".into());
            }
            for p in b.m.get_model().get_all_cells() {
                if scope != "pasta" && p.index != s
                    || scope == "seleção" && !a.contains(p.row, p.column)
                {
                    continue;
                }
                let raw = b.raw(p.index, p.row, p.column);
                if raw.contains(old) {
                    b.set(p.index, p.row, p.column, &raw.replace(old, new))?
                }
            }
            Ok(())
        }
        "negrito" | "italico" | "sublinhado" | "quebra" => {
            let st = b.m.get_cell_style(s, r, c)?;
            let (k, on) = match op {
                "negrito" => ("font.b", st.font.b),
                "italico" => ("font.i", st.font.i),
                "sublinhado" => ("font.u", st.font.u),
                _ => (
                    "alignment.wrap_text",
                    st.alignment.is_some_and(|v| v.wrap_text),
                ),
            };
            style(b, s, a, k, if on { "false" } else { "true" })
        }
        "cor" | "fundo" => {
            if arg.len() != 7
                || !arg.starts_with('#')
                || !arg[1..].bytes().all(|v| v.is_ascii_hexdigit())
            {
                return Err("Use #RRGGBB".into());
            }
            style(
                b,
                s,
                a,
                if op == "cor" {
                    "font.color"
                } else {
                    "fill.color"
                },
                arg,
            )
        }
        "esquerda" | "centro" | "direita-alinhar" => style(
            b,
            s,
            a,
            "alignment.horizontal",
            match op {
                "centro" => "center",
                "esquerda" => "left",
                _ => "right",
            },
        ),
        "borda" => {
            let border = serde_json::from_value(
                serde_json::json!({"item":{"style":"thin","color":"#80909D"},"type":"All"}),
            )
            .map_err(|e| e.to_string())?;
            b.m.set_area_with_border(&area, &border)
        }
        "formato" => {
            let (k, p) = split(arg);
            let fmt = match k {
                "geral" => "general",
                "numero" => "#,##0.00",
                "moeda" => "\"R$\" #,##0.00",
                "data" => "dd/mm/yyyy",
                "hora" => "hh:mm:ss",
                "percentual" => "0.00%",
                "texto" => "@",
                "personalizado" if !p.is_empty() => p,
                _ => return Err("Formato desconhecido".into()),
            };
            style(b, s, a, "num_fmt", fmt)
        }
        "mesclar" => {
            for rr in a.r..=a.end {
                for cc in a.c..=a.right {
                    if (rr, cc) != (a.r, a.c) && !b.raw(s, rr, cc).is_empty() {
                        return Err(
                            "Mescla descartaria conteúdo. Mova ou limpe as outras células".into(),
                        );
                    }
                }
            }
            b.m.merge_cells(&area)
        }
        "desmesclar" => b.m.unmerge_cells(&area),
        "ordenar" => {
            let (header, keys) = split(arg);
            if !matches!(header, "cab" | "sem") {
                return Err("Indique cab (cabeçalho) ou sem".into());
            }
            let keys = keys
                .split(',')
                .map(|key| {
                    Ok((
                        column_to_number(&key.trim_end_matches(['+', '-']).to_uppercase())?,
                        key.ends_with('-'),
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            b.sort_rows(s, a, &keys, header == "cab")
        }
        "filtrar" => {
            let (col, cond) = split(arg);
            b.filter(s, a, column_to_number(&col.to_uppercase())?, cond)
        }
        "remover-filtro" => b.unfilter(s),
        "tabela" => b.create_table(s, a, arg),
        "coluna-calculada" => b.table_formula(s, r, c, arg),
        "registro" => b.extend_table(s, r, c, false),
        "totais" => b.toggle_totals(s, r, c),
        "redimensionar-tabela" => b.resize_table(s, r, c, parse_range(arg)?),
        "renomear-tabela" | "renomear-coluna" => {
            b.rename_table_part(s, r, c, arg, op == "renomear-coluna")
        }
        "converter-intervalo" => b.convert_table(s, r, c),
        "validar" => {
            let (kind, args) = split(arg);
            let list = if kind == "lista" {
                args.split(',').map(str::to_owned).collect::<Vec<_>>()
            } else {
                args.split_whitespace().map(str::to_owned).collect()
            };
            let ok = match kind {
                "lista" => !args.is_empty() && !args.contains('"') && args.len() < 254,
                "numero" => {
                    list.len() == 2
                        && list
                            .iter()
                            .all(|v| v.replace(',', ".").parse::<f64>().is_ok())
                }
                "data" => list.len() == 2 && list.iter().all(|v| iso_date(v)),
                _ => false,
            };
            if !ok {
                return Err("Regra de validação inválida".into());
            }
            b.extra.rules.push(Rule {
                sheet: id,
                area: a,
                kind: kind.into(),
                args: list,
            });
            Ok(())
        }
        "remover-validacao" => {
            b.extra.rules.retain(|v| v.sheet != id || v.area != a);
            Ok(())
        }
        "condicional" => {
            let mut args = arg.split_whitespace();
            let format = Dxf {
                fill: Some(Fill {
                    color: Color::Rgb("#CF8043".into()),
                }),
                ..Dxf::default()
            };
            let rule = match args.next() {
                Some("duplicados") => CfRuleInput::DuplicateValues {
                    format,
                    stop_if_true: false,
                },
                Some(op @ ("maior" | "menor" | "entre")) => {
                    let formula = args.next().ok_or("Falta limite")?.into();
                    let formula2 = if op == "entre" {
                        Some(args.next().ok_or("Falta limite superior")?.into())
                    } else {
                        None
                    };
                    CfRuleInput::CellIs {
                        operator: match op {
                            "maior" => ValueOperator::GreaterThan,
                            "menor" => ValueOperator::LessThan,
                            _ => ValueOperator::Between,
                        },
                        formula,
                        formula2,
                        format,
                        stop_if_true: false,
                    }
                }
                _ => return Err("Use maior, menor, entre ou duplicados".into()),
            };
            b.m.add_conditional_formatting(s, &a.address(), rule)
        }
        "nome" | "excluir-nome" => {
            let (scope, name) = split(arg);
            let scope = match scope {
                "aba" => Some(s),
                "pasta" => None,
                _ => return Err("Use nome aba|pasta identificador".into()),
            };
            if op == "excluir-nome" {
                b.m.delete_defined_name(name, scope)
            } else {
                let absolute = |r, c| format!("${}${r}", number_to_column(c).unwrap());
                let reference = format!(
                    "'{}'!{}:{}",
                    b.ws(s).name.replace('\'', "''"),
                    absolute(a.r, a.c),
                    absolute(a.end, a.right)
                );
                b.m.new_defined_name(name, scope, &reference)
            }
        }
        "nota" | "remover-nota" => {
            b.m.set_note(s, &addr(r, c), if op == "nota" { arg } else { "" })
        }
        "link" => b.m.set_cell_link(
            s,
            r,
            c,
            Link::External {
                target: arg.into(),
                tooltip: None,
            },
            None,
        ),
        _ => Err(format!("Ação desconhecida: {op}")),
    }
}
