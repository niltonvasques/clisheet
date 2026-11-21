"""Verifica OOXML e caches com leitor independente, sem usar IronCalc."""
from pathlib import Path
import sys
from openpyxl import load_workbook
from openpyxl.chart import BarChart

EXAMPLES = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parents[1] / 'examples/xlsx'
wb = load_workbook(EXAMPLES / 'vendas.xlsx')
ws = wb['Vendas']
tb = ws.tables['tbVendas']
assert tb.ref == 'A1:D4'
assert tb.autoFilter.ref == 'A1:D3'
assert tb.tableStyleInfo.name == 'TableStyleMedium2'
assert tb.totalsRowCount == 1
assert tb.tableColumns[3].totalsRowFunction == 'sum'
assert tb.tableColumns[3].calculatedColumnFormula.attr_text == '[@Quantidade]*[@Preco]'
assert ws['D2'].value == '=[@Quantidade]*[@Preco]'
assert ws['D3'].value == '=[@Quantidade]*[@Preco]'
assert ws['F1'].value == '=SUM(tbVendas[Total])'
assert ws['D4'].value == '=SUBTOTAL(109,tbVendas[Total])'
cached = load_workbook(EXAMPLES / 'vendas.xlsx', data_only=True).active
assert [cached[c].value for c in ['D2','D3','D4','F1']] == [20,15,35,35]
ws = load_workbook(EXAMPLES / 'orcamento.xlsx').active
assert ws['C2'].data_type == 'n' and ws['C2'].value == 420.5
assert 'R$' in ws['C2'].number_format and ws['A1'].font.bold
ws = load_workbook(EXAMPLES / 'resumo.xlsx')
assert ws.sheetnames == ['Resumo','Preços 2026']
assert ws['Resumo']['A1'].value == "='Preços 2026'!B2*2"
ws = load_workbook(EXAMPLES / 'incompativel.xlsx').active
assert isinstance(ws._charts[0], BarChart)
print('openpyxl conferiu tipos, fórmulas, caches, estilo, metadados da tabela e o gráfico externo.')
