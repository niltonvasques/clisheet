# Medições de desempenho

Execução local em Ubuntu 24.04, Linux x86_64, Intel Core i9-9900KS, 16 CPUs lógicas. Build Rust em release. Para reproduzir na raiz do projeto:

```sh
cargo run --release --locked --example benchmark
```

| Operação | 100.000 números + 10.000 fórmulas | A1 e XFD1048576 |
|---|---:|---:|
| Construção em uma transação | 149,30 ms | 0,06 ms |
| Salvar XLSX e validar | 218,44 ms | 7,92 ms |
| Abrir XLSX e calcular | 98,46 ms | 0,50 ms |
| Editar A1 com histórico e recálculo | 5,45 ms | 0,04 ms |
| Recalcular a pasta | 4,45 ms | 0,01 ms |
| Navegar e desenhar, p50 | 0,42 ms | 0,24 ms |
| Navegar e desenhar, p95 | 0,42 ms | 0,25 ms |

O caso denso ocupa A1:K10000 com dez números por linha e uma soma das colunas A e J. O caso esparso alterna o foco entre regiões distantes da mesma aba. O benchmark executa 201 navegações e descarta a primeira. Os percentis incluem a atualização do foco e a renderização de 100×30 no TestBackend. A meta de 50 ms foi alcançada nessa medida. Latência de SSH, emulador e desenho físico da tela precisam de medição separada. Os demais tempos correspondem a uma execução de cada operação. O pico do processo completo, informado pelo kernel em VmHWM, foi **78.060 KiB**.
