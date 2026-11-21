use super::*;
use std::ops::Range as Span;

pub(super) struct Suggestions {
    options: Vec<&'static str>,
    command: String,
    selected: usize,
    span: Span<usize>,
}

impl Suggestions {
    fn for_editor(editor: &Editor) -> Self {
        let before = &editor.text[..editor.cursor];
        let start = before.trim_end_matches(|c: char| !c.is_whitespace()).len();
        let end = editor.cursor
            + editor.text[editor.cursor..]
                .find(char::is_whitespace)
                .unwrap_or(editor.text.len() - editor.cursor);
        let words: Vec<_> = before[..start].split_whitespace().collect();
        let actions = ACTIONS.iter().flat_map(|group| group.split('|'));
        let mut options: Vec<_> = match words.as_slice() {
            [] => actions
                .filter_map(|a| a.split_whitespace().next())
                .collect(),
            ["importar"] | ["exportar", "seleção" | "aba"] => {
                vec!["csv", "virgula", "ponto-e-virgula", "tab", "tsv"]
            }
            ["importar", _] | ["exportar", _, _] => vec!["latin1", "utf-8"],
            [cmd @ ("buscar" | "colar" | "formato" | "condicional" | "validar" | "ordenar"
            | "substituir" | "nome" | "excluir-nome" | "exportar")] => {
                let mut values: Vec<_> = actions
                    .filter_map(|a| {
                        let mut parts = a.split_whitespace();
                        if parts.next() == Some(*cmd) {
                            parts.next()
                        } else {
                            None
                        }
                    })
                    .collect();
                if *cmd == "condicional" {
                    values.push("menor")
                }
                values
            }
            _ => Vec::new(),
        };
        let prefix = before[start..].to_lowercase();
        options.retain(|v| v.starts_with(&prefix));
        options.sort_unstable();
        options.dedup();
        Self {
            options,
            command: words.first().copied().unwrap_or_default().to_owned(),
            selected: 0,
            span: start..end,
        }
    }

    pub(super) fn visible(&self, height: u16) -> (u16, usize, usize) {
        let count = self
            .options
            .len()
            .min(8)
            .min(height.saturating_sub(4) as usize);
        (
            height.saturating_sub(1 + count as u16),
            (self.selected + 1).saturating_sub(count),
            count,
        )
    }

    pub(super) fn description_column(&self) -> u16 {
        5 + self
            .options
            .iter()
            .map(|word| word.width())
            .max()
            .unwrap_or(0) as u16
    }

    pub(super) fn rows(&self, height: u16) -> impl Iterator<Item = (u16, bool, &str, &str)> {
        let (top, offset, count) = self.visible(height);
        self.options
            .iter()
            .enumerate()
            .skip(offset)
            .take(count)
            .map(move |(i, opt)| {
                (
                    top + (i - offset) as u16,
                    i == self.selected,
                    *opt,
                    description(&self.command, opt),
                )
            })
    }
}

impl App {
    fn accept_suggestion(&mut self) {
        if let Some(menu) = self.suggestions.take() {
            let word = menu.options[menu.selected];
            let takes_arguments = self.editor.text[..menu.span.start].trim().is_empty()
                && ACTIONS
                    .iter()
                    .flat_map(|group| group.split('|'))
                    .any(|a| a.split_once(' ').is_some_and(|(name, _)| name == word));
            self.editor.cursor = menu.span.end;
            self.editor.anchor = Some(menu.span.start);
            self.editor.replace(word);
            if takes_arguments && self.editor.cursor == self.editor.text.len() {
                self.editor.replace(" ")
            }
        }
    }

    pub(super) fn completion_key(&mut self, k: KeyEvent) -> bool {
        if let Some(menu) = self.suggestions.as_mut() {
            let n = menu.options.len();
            match k.code {
                K::BackTab | K::Up => menu.selected = (menu.selected + n - 1) % n,
                K::Tab if k.modifiers.contains(M::SHIFT) => {
                    menu.selected = (menu.selected + n - 1) % n
                }
                K::Tab | K::Down => menu.selected = (menu.selected + 1) % n,
                K::Enter => self.accept_suggestion(),
                K::Esc => self.suggestions = None,
                _ => {
                    self.suggestions = None;
                    return false;
                }
            }
            return true;
        }
        if !matches!(k.code, K::Tab | K::BackTab) {
            return false;
        }
        let menu = Suggestions::for_editor(&self.editor);
        let count = menu.options.len();
        if count > 0 {
            self.suggestions = Some(menu);
            if count == 1 {
                self.accept_suggestion()
            }
        }
        true
    }

    pub(super) fn completion_mouse(&mut self, m: MouseEvent) -> bool {
        let Some(menu) = self.suggestions.as_mut() else {
            return false;
        };
        let (top, offset, count) = menu.visible(self.height);
        match m.kind {
            MK::Down(B::Left) if (top..top + count as u16).contains(&m.row) => {
                menu.selected = offset + usize::from(m.row - top);
                self.accept_suggestion();
            }
            MK::ScrollUp => menu.selected = menu.selected.saturating_sub(1),
            MK::ScrollDown => menu.selected = (menu.selected + 1).min(menu.options.len() - 1),
            MK::Down(_) => self.suggestions = None,
            _ => (),
        }
        true
    }
}

fn description(command: &str, word: &str) -> &'static str {
    if !command.is_empty() {
        return match (command, word) {
            ("colar", "formato") => "Cola apenas a formatação",
            ("colar", "formulas") => "Cola apenas as fórmulas",
            ("colar", "valores") => "Cola apenas os valores",
            ("colar", "transpor") => "Troca linhas por colunas ao colar",
            ("buscar", "valor") => "Procura no valor exibido",
            ("buscar", "original") => "Procura no conteúdo original",
            ("buscar", "formula") => "Procura no texto das fórmulas",
            ("ordenar", "cab") => "Mantém a primeira linha como cabeçalho",
            ("ordenar", "sem") => "Inclui a primeira linha na ordenação",
            ("validar", "numero") => "Aceita números entre dois limites",
            ("validar", "data") => "Aceita datas entre dois limites",
            ("validar", "lista") => "Aceita os itens da lista",
            ("formato", "data") => "Exibe dia, mês e ano",
            ("formato", "hora") => "Exibe horas, minutos e segundos",
            ("formato", "numero") => "Exibe milhares e duas casas decimais",
            ("formato", "percentual") => "Exibe o valor em porcentagem",
            ("formato", "moeda") => "Exibe o valor em reais",
            ("formato", "geral") => "Usa o formato geral",
            ("formato", "texto") => "Aplica o formato de texto",
            ("formato", "personalizado") => "Usa o código de formato informado",
            ("condicional", "duplicados") => "Destaca valores repetidos",
            ("condicional", "maior") => "Destaca valores acima do limite",
            ("condicional", "menor") => "Destaca valores abaixo do limite",
            ("condicional", "entre") => "Destaca valores entre dois limites",
            (_, "aba") => "Usa apenas a aba atual",
            (_, "pasta") => "Usa toda a pasta de trabalho",
            (_, "seleção") => "Usa apenas as células selecionadas",
            (_, "csv" | "virgula") => "Separa campos por vírgula",
            (_, "ponto-e-virgula") => "Separa campos por ponto e vírgula",
            (_, "tab" | "tsv") => "Separa campos por tabulação",
            (_, "latin1") => "Usa a codificação ISO-8859-1",
            (_, "utf-8") => "Usa a codificação UTF-8",
            _ => "",
        };
    }
    match word {
        "abrir" => "Abre um arquivo XLSX",
        "salvar" => "Salva a pasta de trabalho",
        "copia" => "Salva uma cópia em outro arquivo",
        "importar" => "Lê dados de um CSV ou TSV",
        "exportar" => "Grava valores em CSV ou TSV",
        "sair" => "Fecha o clisheet",
        "editar" => "Edita a célula ativa",
        "desfazer" => "Desfaz a última alteração",
        "refazer" => "Reaplica a alteração desfeita",
        "copiar" => "Copia a seleção para colar no clisheet",
        "copiar-externo" => "Copia a seleção para o clipboard externo",
        "recortar" => "Marca a seleção para mover ao colar",
        "colar" => "Cola o bloco do clipboard interno",
        "limpar" => "Apaga o conteúdo e mantém o formato",
        "limpar-formato" => "Remove a formatação da seleção",
        "limpar-tudo" => "Apaga conteúdo, formato e notas",
        "preencher" => "Repete um valor na seleção",
        "baixo" => "Preenche a partir da primeira linha",
        "direita" => "Preenche a partir da primeira coluna",
        "serie" => "Preenche uma série com o passo indicado",
        "buscar" => "Localiza a próxima ocorrência na aba",
        "substituir" => "Troca um texto por outro",
        "ir" => "Vai para uma célula ou intervalo",
        "congelar" => "Fixa linhas acima e colunas à esquerda",
        "descongelar" => "Libera as linhas e colunas fixas",
        "largura" => "Define a largura das colunas",
        "altura" => "Define a altura das linhas",
        "autoajustar" => "Ajusta a largura da coluna ao conteúdo",
        "ocultar-linhas" => "Oculta as linhas selecionadas",
        "mostrar-linhas" => "Reexibe as linhas selecionadas",
        "ocultar-colunas" => "Oculta as colunas selecionadas",
        "mostrar-colunas" => "Reexibe as colunas selecionadas",
        "proxima-aba" => "Vai para a aba seguinte",
        "aba-anterior" => "Vai para a aba anterior",
        "tema" => "Alterna entre tema claro e escuro",
        "nova-aba" => "Cria uma aba vazia",
        "renomear-aba" => "Altera o nome da aba atual",
        "duplicar-aba" => "Cria uma cópia da aba atual",
        "excluir-aba" => "Exclui a aba atual",
        "mover-aba" => "Muda a posição da aba atual",
        "ocultar-aba" => "Oculta a aba atual",
        "mostrar-aba" => "Reexibe uma aba pelo nome",
        "inserir-linhas" => "Insere linhas na seleção",
        "inserir-colunas" => "Insere colunas na seleção",
        "excluir-linhas" => "Exclui as linhas selecionadas",
        "excluir-colunas" => "Exclui as colunas selecionadas",
        "link" => "Define um hiperlink na célula",
        "nota" => "Escreve uma nota na célula",
        "ler-nota" => "Mostra a nota da célula",
        "remover-nota" => "Apaga a nota da célula",
        "negrito" => "Alterna o negrito na seleção",
        "italico" => "Alterna o itálico na seleção",
        "sublinhado" => "Alterna o sublinhado na seleção",
        "borda" => "Aplica bordas às células",
        "quebra" => "Alterna a quebra de texto",
        "esquerda" => "Alinha o conteúdo à esquerda",
        "centro" => "Centraliza o conteúdo",
        "direita-alinhar" => "Alinha o conteúdo à direita",
        "formato" => "Define como os valores são exibidos",
        "cor" => "Muda a cor do texto",
        "fundo" => "Muda a cor de fundo das células",
        "mesclar" => "Une as células da seleção",
        "desmesclar" => "Separa as células mescladas",
        "condicional" => "Destaca células que atendem a uma regra",
        "ordenar" => "Ordena os registros da seleção",
        "filtrar" => "Exibe as linhas que atendem ao filtro",
        "remover-filtro" => "Limpa o filtro da aba",
        "tabela" => "Cria uma tabela a partir da seleção",
        "converter-intervalo" => "Converte a tabela em intervalo",
        "redimensionar-tabela" => "Altera o intervalo da tabela",
        "coluna-calculada" => "Define a fórmula da coluna da tabela",
        "registro" => "Acrescenta uma linha à tabela",
        "totais" => "Alterna a linha de totais da tabela",
        "renomear-tabela" => "Altera o nome da tabela",
        "renomear-coluna" => "Altera o nome da coluna da tabela",
        "validar" => "Restringe os valores aceitos nas células",
        "remover-validacao" => "Remove a validação do intervalo",
        "recalcular" => "Atualiza os resultados das fórmulas",
        "nome" => "Dá um nome ao intervalo selecionado",
        "nomes" => "Lista os nomes definidos",
        "excluir-nome" => "Remove um nome definido",
        "ajuda" => "Mostra a ajuda e os atalhos",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_suggested_command_and_argument_has_help() {
        for action in ACTIONS.iter().flat_map(|group| group.split('|')) {
            let mut prefix = String::new();
            for word in action.split_whitespace() {
                let menu = Suggestions::for_editor(&Editor::new(&prefix));
                for option in menu.options {
                    assert!(
                        !description(&menu.command, option).is_empty(),
                        "{prefix}{option}"
                    );
                }
                prefix.push_str(word);
                prefix.push(' ');
            }
        }
    }
}
