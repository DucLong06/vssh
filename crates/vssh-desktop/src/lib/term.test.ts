import { expect, test } from 'vitest'
import { lastOutput, osc52 } from './term'

/** A screen after `text` was printed: its lines, and the cursor at the end. */
function last(text: string) {
  const rows = text.split('\r\n')
  return lastOutput(rows, rows.length - 1, rows[rows.length - 1].length)
}

// The cases of the TUI's own test (crates/vssh-tui/src/ui/term.rs).
test('the last output is what lies between two prompts', () => {
  const session = 'web:~# ls\r\nold  news\r\nweb:~# df -h /\r\nFilesystem  Use%\r\n/dev/sda1   61%\r\n\r\nweb:~# '
  expect(last(session)).toBe('Filesystem  Use%\n/dev/sda1   61%')
  // First command of the session: nothing above but the output.
  expect(last('web:~# uptime\r\n up 3 days\r\nweb:~# ')).toBe(' up 3 days')
  // Something typed at the prompt, or a prompt that changes: everything above the cursor.
  expect(last('web:~# ls\r\na  b\r\nweb:~# ech')).toBe('web:~# ls\na  b')
  expect(last('10:01 $ ls\r\na  b\r\n10:02 $ ')).toBe('10:01 $ ls\na  b')
  expect(last('')).toBe('')
  expect(last('web:~# ')).toBe('')
})

test('lines below the cursor are not output', () => {
  expect(lastOutput(['$ ls', 'a  b', '$ ', '', ''], 2, 2)).toBe('a  b')
})

test('an OSC 52 from the remote side carries text for the clipboard, and never reads it', () => {
  expect(osc52(`c;${btoa('ssh web-01')}`)).toBe('ssh web-01')
  expect(osc52(`;${btoa('no selection named')}`)).toBe('no selection named')
  // UTF-8, as the TUI sends it.
  const bytes = new TextEncoder().encode('máy chủ')
  expect(osc52(`c;${btoa(String.fromCharCode(...bytes))}`)).toBe('máy chủ')
  expect(osc52('c;?')).toBeNull()
  expect(osc52('c;not base64!')).toBeNull()
  expect(osc52(btoa('no semicolon'))).toBeNull()
  expect(osc52(`c;${btoa('\xff\xfe')}`)).toBeNull()
})
