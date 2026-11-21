# Compatibilidade XLSX

O aplicativo identifica seus pacotes por uma impressão digital do conteúdo. Ctrl+S reutiliza o caminho de um XLSX externo quando o pacote contém uma grade simples reconhecida. Partes ou recursos fora dessa cobertura exigem um destino novo. Em **Salvar cópia**, o original continua contendo todos os objetos. A coluna **Gravação** descreve o conteúdo da cópia ou de um arquivo produzido pelo clisheet. “Parcial” delimita os recursos aceitos, “sem ensaio” identifica falta de verificação específica, e “—” indica que aquela ação não se aplica.

| Recurso | Leitura | Exibição | Edição | Cálculo | Gravação | Limite ou evidência |
|---|---|---|---|---|---|---|
| Números, booleanos, Unicode, texto vazio, zero, identificadores com zeros | Sim | Sim | Sim | Tipado | Sim | Teste `typed_roundtrip_and_failure`; apóstrofo força texto |
| Datas, horas, percentuais e moeda no sistema 1900 | Sim | Por formato | Sim | Número serial | Sim | Data ISO e percentual no round-trip; orçamento conferido com openpyxl. Seriais 1, 59, 60 e 61 verificados em `excel_1900_date_boundary`, inclusive TEXT e DAY no dia fictício |
| Sistema de datas 1904 | Detecta | — | — | — | Original protegido | Abertura recusada explicitamente em `date_epoch_and_chart_protection` |
| Fórmulas escalares, dependências e referências entre abas | Sim | Valor e expressão | Sim | Recálculo completo | Fórmula e resultado | `formulas`, `calculation_history_and_savepoint`, `structure_and_failed_batch` e PTY |
| Fórmulas compartilhadas | Sim | Sim | Sim | Sim | Fórmulas equivalentes por célula | `shared_formulas_are_calculated_and_reexported` edita um precedente do seguidor e reabre |
| Referências mistas, intervalos e nomes | Sim | Sim | Sim | Sim | Pelo motor | Cópia horizontal e vertical em `copy_cut`; mover entre abas em `note_and_cross_sheet_move`. Combinações de nomes e edições estruturais têm cobertura parcial |
| Erros, ciclos e função desconhecida | Parcial | Erro indicado | Sim | Conforme motor | Cobertura parcial | Testes de divisão por zero, referência circular e função desconhecida. Cache externo recalculado ao abrir. Preservação de caches antigos e expressões modernas desconhecidas sem garantia geral |
| Tabelas com cabeçalho, coluna calculada e totais | Sim, básicas | Dados, filtro e totais | Criar, ampliar, ordenar, renomear, converter | Sim | Partes de tabela OOXML | `tables`, `table_commands_and_structural_undo` e openpyxl conferem D2=20, D3=15, total=35, referência e fórmula da coluna |
| Referências estruturadas | Parcial | Expressão original e valor | Formas básicas | `tbVendas[Total]` e `[@Quantidade]` | Fórmulas simbólicas | Especificadores avançados, colunas especiais e referências externas têm cobertura parcial |
| Filtros de região e de tabela | Parcial | Linhas visíveis e indicador | Um critério ativo por aba | SUBTOTAL diferencia linhas filtradas das ocultas manualmente | Critério e visibilidade básicos | Teste `filters_subtotal_and_sort`. Em arquivo externo a origem da ocultação é inferida do critério |
| Formatos e estilos | Parcial | Cor, atributos, alinhamento, formato e quebra no espaço disponível | Intervalos | Aplicação de formato preserva o valor | Estilos básicos | Orçamento validado com openpyxl. Temas, bordas e estilos do Excel têm fidelidade visual parcial |
| Dimensões, mesclas, congelamento e abas | Pelo motor | Grade e abas | Sim | — | Pelo motor | openpyxl conferiu B2 congelada e C1:D1 mescladas; há testes de navegação com congelamento, resize e operações de aba. Cobertura de variantes externas parcial |
| Formatação condicional | Pelo motor | Parcial | Comparação, entre e duplicados | Pelo motor | Pelo motor | openpyxl conferiu a regra entre 2 e 6 e seu intervalo após edição por PTY; variantes avançadas sem ensaio |
| Validações | Lista literal, limites numéricos e de datas | Erro na edição | Regras por intervalo | Validação de entrada | Formas básicas em dataValidations | openpyxl verificou limites e endereço após salvar. Listas por fórmula e validações personalizadas externas sem fidelidade garantida |
| Hiperlinks e notas clássicas | Parcial | Consulta de nota | Criar/remover nota, definir link | — | Texto de nota e vínculo básico | `note_and_cross_sheet_move` reabre a nota. Texto e autoria de nota clássica sobrevivem a ordenar, mover e reabrir em `notes_follow_records_and_preserve_authorship`. Texto rico e comentários encadeados sem garantia |
| Gráficos, imagens, desenhos, pivôs, conexões | Parcial, conforme o pacote | — | — | — | Podem ser omitidos da cópia | `date_epoch_and_chart_protection` verifica byte a byte a proteção do original com gráfico; openpyxl confirma o objeto no exemplo |
| XLS, XLSB, XLSM, criptografia | Recusados | — | — | — | — | Somente caminho .xlsx e pacote reconhecido; macros e DOCTYPE recusados |

## Fórmulas e operações

A suíte `tests/product.rs` contém as expressões e os resultados esperados para as funções obrigatórias de agregação, lógica, condicionais, matemática, texto, busca, datas e informação de P0 e P1. Inclui IF com ramo inativo contendo divisão por zero, XLOOKUP, NETWORKDAYS e SUMPRODUCT. Outras funções disponíveis na versão fixada do motor podem ser usadas. A suíte do aplicativo não cobre todos os modos opcionais dessas funções, funções financeiras, matrizes dinâmicas, LET ou LAMBDA.

Na ordenação, selecione o registro completo e indique as colunas de desempate. Em ordem crescente, o aplicativo agrupa números, outros valores exibidos, erros identificados pelo prefixo # e vazios. A ordem decrescente inverte o critério. Linhas empatadas preservam a ordem de origem. As fórmulas acompanham o registro e deslocam suas referências relativas como numa cópia. A operação pede ampliação quando encontra dados contíguos fora da largura selecionada.

Algumas combinações estruturais são bloqueadas para preservar coerência, incluindo recorte envolvendo tabelas ou validações, quando a área de origem ou destino intercepta esses metadados. Fórmulas que combinam referências de tabela e de célula acompanham o recorte entre abas em `mixed_table_dependencies_follow_a_cut_between_sheets`. Desative os totais antes de redimensionar uma tabela. A interface pt-BR oferece aliases básicos de função. Tradução integral da interface para inglês e testes de interoperabilidade com todos os recursos importados seguem pendentes.

## Evidência executada

Os testes Rust exercitam transações, XLSX, falhas de gravação e eventos de interface. O teste PTY Linux também envia eventos de mouse no protocolo SGR e confere seus efeitos no arquivo. Ele abre a TUI em 80×24, edita sem mouse, salva, reabre, recupera uma sessão encerrada abruptamente e confere o arquivo original. openpyxl 3.1.2 verifica expressões, caches, tipos e XML de tabela por um leitor independente. O ambiente tinha LibreOffice sem o componente Calc, portanto a comparação com Calc ficou indisponível. Excel, Arch, macOS, Windows e uma sessão SSH real ainda não foram verificados.


O calendário 1900 reproduz o dia fictício de número 60 ao apresentar a data e ao extrair seus componentes. Funções financeiras e aritmética avançada envolvendo fevereiro de 1900 têm cobertura parcial. [Calendário do Excel, Microsoft](https://learn.microsoft.com/pt-br/troubleshoot/microsoft-365-apps/excel/wrongly-assumes-1900-is-leap-year)

## Estado de P0 e P1

- **P0 utilizável**. Edição, seleção por teclado e mouse, referências relativas/mistas, cálculo local, múltiplas abas, transações, busca, ordenação e filtros básicos foram exercitados em Linux. As limitações incluem lotes densos de até 500.000 células por operação e um critério de filtro ativo por aba. O indicador de progresso enfileira entrada nas operações despachadas em segundo plano. A abertura inicial ocorre antes do primeiro desenho.
- **P1 implementado e ensaiado**. Tabelas OOXML com fórmula e total, conversão em intervalo, recorte entre abas, colagens especiais, referência a outra aba durante a edição, estilos e validações básicos, notas e recuperação. A coluna de cálculo das tabelas usa as formas simbólicas indicadas na matriz. Fórmulas de coluna com operadores estruturados avançados, variações de estilos e caches de fórmulas de outros produtores ainda exigem testes específicos.
- **Interoperabilidade pendente**. Edição no sistema 1904, preservação de objetos externos na cópia, paridade de todas as operações em regiões com tabelas ou validações, e comparação efetiva com Excel/Calc. Os exemplos, a proteção de originais e a leitura independente cobrem a faixa de formatos demonstrada.
