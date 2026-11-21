"""Fluxo real em PTY Linux. Usa apenas biblioteca padrão, openpyxl e o binário compilado."""
import errno
import fcntl
import os
from pathlib import Path
import pty
import select
import shutil
import signal
import struct
import subprocess
import tempfile
import termios
import time

from openpyxl import load_workbook

ROOT = Path(__file__).resolve().parents[1]
BIN = Path(os.environ.get("CLISHEET_BIN", ROOT / "target/debug/clisheet"))


class Session:
    def __init__(self, args, state):
        self.master, self.slave = pty.openpty()
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 80, 0, 0))
        self.before = termios.tcgetattr(self.slave)
        self.proc = subprocess.Popen([BIN, *args], stdin=self.slave, stdout=self.slave,
            stderr=self.slave, env=dict(os.environ, TERM='xterm-256color', XDG_STATE_HOME=str(state)))
        self.wait_for(b'clisheet')

    def wait_for(self, text, timeout=5):
        end = time.monotonic() + timeout
        out = b''
        while time.monotonic() < end:
            if select.select([self.master], [], [], .1)[0]:
                out += os.read(self.master, 65536)
            if text in out:
                return
        raise AssertionError((text, out[-1200:]))

    def send(self, keys):
        os.write(self.master, keys)
        time.sleep(.05)

    def finish(self):
        try:
            assert self.proc.wait(timeout=8) == 0
            assert termios.tcgetattr(self.slave) == self.before
        finally:
            if self.proc.poll() is None:
                self.proc.kill()
            os.close(self.master)
            os.close(self.slave)


with tempfile.TemporaryDirectory() as tmp:
    root = Path(tmp)
    path = root / 'teste.xlsx'
    shutil.copyfile(ROOT / 'examples/xlsx/resumo.xlsx', path)
    args = [path, '--sheet', 'Preços 2026', '--no-mouse']
    session = Session(args, root)
    session.send(b':ir B2\r15\r')
    session.send(b'\x13')
    session.wait_for(b'Salvo em')
    session.send(b'\x11')
    session.finish()
    assert load_workbook(path, data_only=True)['Resumo']['A1'].value == 30
    assert load_workbook(path)['Resumo']['A1'].value == "='Preços 2026'!B2*2"
    reopened = Session([path, '--sheet', 'Resumo', '--read-only'], root)
    reopened.send(b'\x11')
    reopened.finish()
    print('PTY: abriu → editou B2 de 10 para 15 → calculou 30 → salvou → reabriu.')

    changed = Session(args, root)
    changed.send(b':ir B2\r25\r')
    # Wait for a complete recovery file to appear (atomic publish).
    for _ in range(60):
        if list(root.glob('clisheet/sessao-*.xlsx')):
            break
        time.sleep(.1)
    else:
        raise AssertionError('Snapshot ausente')
    changed.proc.kill()
    changed.proc.wait()
    os.close(changed.master)
    os.close(changed.slave)
    recovered = Session(['--recover', '--no-mouse'], root)
    recovered.send(b'\x13')
    recovered.send(str(root / 'recuperado.xlsx').encode() + b'\r')
    recovered.wait_for(b'Salvo em')
    recovered.send(b'\x11')
    recovered.finish()
    assert load_workbook(root/'recuperado.xlsx', data_only=True)['Resumo']['A1'].value == 50
    assert load_workbook(path, data_only=True)['Resumo']['A1'].value == 30
    print('PTY: snapshot após encerramento abrupto → --recover → cópia com total 50.')

    # SGR mouse input uses one-based terminal coordinates. Select A1:A3 by
    # dragging, then scroll three rows and double click the now-visible A4.
    mouse_path = root / 'mouse.xlsx'
    sheet = Session([], root)
    sheet.send(b'\x1b[<0;8;5M\x1b[<32;8;7M\x1b[<0;8;7m')
    sheet.send(b':preencher 4\r')
    sheet.send(b'\x1b[<65;8;8M')
    sheet.send(b'\x1b[<0;8;5M\x1b[<0;8;5m\x1b[<0;8;5M\x1b[<0;8;5m')
    sheet.send(b'9\r')
    sheet.send(b':ir B1\r:nota Conferido\r')
    sheet.send(b':ir A1:A3\r:nome pasta Dados\r')
    sheet.send(b':validar numero 0 10\r:condicional entre 2 6\r')
    sheet.send(b':ir C1:D1\r:mesclar\r')
    sheet.send(b':ir B2\r:congelar\r')
    sheet.send(b'\x13')
    sheet.send(str(mouse_path).encode() + b'\r')
    sheet.wait_for(b'Salvo em')
    sheet.send(b'\x11')
    sheet.finish()
    wb = load_workbook(mouse_path)
    ws = wb.active
    assert [ws.cell(row, 1).value for row in range(1, 5)] == [4, 4, 4, 9]
    assert ws['B1'].comment.text == 'Conferido' and ws['B1'].comment.author == 'clisheet'
    assert 'C1:D1' in ws.merged_cells and ws.freeze_panes == 'B2'
    assert wb.defined_names['Dados'].attr_text.endswith('!$A$1:$A$3')
    validation = ws.data_validations.dataValidation[0]
    assert str(validation.sqref) == 'A1:A3' and validation.formula2 == '10'
    rule = ws.conditional_formatting['A1:A3'][0]
    assert rule.operator == 'between' and rule.formula == ['2', '6']
    print('PTY: clique, arrasto, rolagem e duplo clique. openpyxl conferiu notas, nome, validação, mescla, painéis e regra condicional.')

    from openpyxl import Workbook
    external = root / 'externo.xlsx'
    wb = Workbook()
    wb.active['A1'] = 10
    wb.save(external)
    simple = Session([external], root)
    for value in (15, 20):
        simple.send(f':ir A1\r{value}\r'.encode())
        simple.send(b'\x13')
        simple.wait_for(b'Salvo em')
        assert load_workbook(external).active['A1'].value == value
    simple.send(b'\x11')
    simple.finish()
    print('PTY: Ctrl+S salvou duas vezes no XLSX externo aberto.')

# Keep the desktop clipboard untouched. Exercise the helper's stdin contract
# and verify the actual OSC 52 bytes written to a remote terminal.
from unittest.mock import patch
from base64 import b64encode

with tempfile.TemporaryDirectory() as tmp:
    root = Path(tmp)
    helper = root / 'wl-copy'
    helper.write_text('#!/bin/sh\ncat > "$TEST_CLIPBOARD"\n')
    helper.chmod(0o755)
    captured = root / 'clipboard.tsv'
    expected = 'Café\t2\nChá\t3'.encode()
    path = ROOT / 'examples/xlsx/vendas.xlsx'
    env = dict(os.environ, WAYLAND_DISPLAY='test', TEST_CLIPBOARD=str(captured),
               PATH=str(root) + os.pathsep + os.environ['PATH'])
    for key in ['SSH_TTY', 'SSH_CONNECTION', 'SSH_CLIENT']:
        env.pop(key, None)
    with patch.dict(os.environ, env, clear=True):
        app = Session([path, '--read-only'], root)
        app.send(b':ir A2:B3\r:copiar-externo\r')
        app.wait_for(b'wl-copy')
        assert captured.read_bytes() == expected
        app.send(b'\x11')
        app.finish()
    with patch.dict(os.environ, SSH_TTY='/dev/pts/test'):
        app = Session([path, '--read-only', '--osc52'], root)
        app.send(b':ir A2:B3\r\x03')
        app.wait_for(b'\x1b]52;c;' + b64encode(expected) + b'\x07')
        app.send(b'\x11')
        app.finish()
    print('PTY: seleção de quatro células enviada como TSV ao auxiliar local e via OSC 52.')

with tempfile.TemporaryDirectory() as tmp:
    app = Session(['--no-mouse'], Path(tmp))
    app.send(b':cop\t')
    app.wait_for('Sugestões'.encode())
    app.send(b'\x1b[B\r')  # choose copiar
    app.send(b'\r')         # run it
    app.wait_for(b'clipboard interno')
    app.send(b':sai\t\r')
    app.finish()
    print('PTY: Tab abriu sugestões, Enter escolheu copiar e :sai + Tab completou sair.')
