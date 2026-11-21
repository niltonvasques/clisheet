# clisheet — pendências de P0 e P1

O binário de release está funcional no Ubuntu 24.04. A suíte Rust passou em 28 testes. A TUI também foi exercitada em pseudoterminal Linux, e openpyxl verificou dados, fórmulas, resultados gravados e metadados. A [matriz XLSX](docs/compatibility.md) detalha a cobertura atual.

## P0 — integridade e uso diário

- [ ] **Datas 1904.** Hoje a abertura é recusada explicitamente. Implementar o sistema de datas por pasta e comparar valores seriais, apresentação, fórmulas e regravação com uma pasta 1904 real. Manter os testes da fronteira de fevereiro de 1900.
- [ ] **Fórmulas externas incompatíveis.** Ampliar os testes com expressões modernas desconhecidas, matrizes e caches antigos. Verificar a expressão após salvar uma cópia, o estado mostrado ao usuário e o comportamento após editar um precedente.
- [ ] **Filtros importados.** Hoje há um critério ativo por aba. Cobrir critérios simultâneos em colunas diferentes e arquivos com linhas ocultas manualmente dentro da região filtrada. Conferir as duas modalidades de SUBTOTAL após limpar os filtros e reabrir.
- [ ] **Operações grandes.** A abertura inicial precede o primeiro desenho e os lotes densos têm limite de 500 mil células. Exibir progresso desde a abertura e testar seleções de linhas e colunas inteiras sobre dados esparsos.
- [ ] **Terminal real.** Medir latência de tecla até atualização visível em um emulador e por SSH. A medição existente usa o backend de teste. Verificar resize durante edição, atalhos interceptados e envio por OSC 52 em terminais que ofereçam o protocolo.

## P1 — operações e compatibilidade

- [ ] **Recorte com metadados.** O recorte entre abas atualiza dependências, mas é bloqueado quando a origem ou o destino intercepta uma tabela ou validação. Implementar os casos de movimento integral e testar sobreposição, desfazer e salvar/reabrir.
- [ ] **Tabelas.** Permitir redimensionar com totais ativos. Ampliar a cobertura para referências estruturadas avançadas e cabeçalhos especiais. Conferir fórmulas de coluna, nomes e totais depois de editar a estrutura em outro leitor.
- [ ] **Estilos e produtividade.** Ampliar os ensaios de bordas, temas, validações por fórmula, regras condicionais importadas, hiperlinks e notas com texto rico. Verificar nomes locais após duplicação, exclusão e reordenação de abas.
- [ ] **Localização.** A interface está em português e a entrada aceita pt-BR e en-US. Centralizar as mensagens para tradução e ampliar a ajuda de argumentos das funções.
- [ ] **Comparação entre aplicativos.** Abrir os exemplos em Excel e LibreOffice Calc e conferir fórmulas, resultados recalculados, filtros, estilos e tabelas. O ambiente verificado tinha LibreOffice sem Calc. Validar também o fluxo de instalação e uso no Arch; macOS e Windows seguem sem teste.

## Preservação de objetos externos

- [ ] Ampliar as amostras externas com imagens, desenhos e pivôs e verificar que o original permanece idêntico após editar e salvar uma cópia. A proteção já foi testada com gráfico. Qualquer ampliação da gravação sobre o original depende de demonstrar a preservação das relações e dos objetos.

## Como retomar

Execute `cargo test --locked`, `cargo build --locked`, `python3 tests/independent.py` e `python3 tests/terminal.py` na raiz. Os scripts Python usam openpyxl. Para repetir o fluxo interativo com duas abas, siga o [README](README.md).
