"""Gera um arquivo externo real com gráfico para exercitar Salvar cópia."""
from pathlib import Path
from openpyxl import Workbook
from openpyxl.chart import BarChart, Reference
import sys
path = Path(sys.argv[1] if len(sys.argv) > 1 else 'examples/xlsx/incompativel.xlsx')
if path.exists():
    raise SystemExit(f'Destino já existe: {path}')
wb = Workbook()
ws = wb.active
ws.title = 'Gráfico'
for row in [('Mês', 'Vendas'), ('Jan', 40), ('Fev', 65)]:
    ws.append(row)
chart = BarChart()
chart.title = 'Vendas mensais'
chart.add_data(Reference(ws, min_col=2, min_row=1, max_row=3), titles_from_data=True)
chart.set_categories(Reference(ws, min_col=1, min_row=2, max_row=3))
ws.add_chart(chart, 'D2')
wb.save(path)
