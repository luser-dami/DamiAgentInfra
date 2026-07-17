import sqlite3
c = sqlite3.connect(r'.brain\index\brain.db')

def show(title, pat):
    print(f'=== {title} ===')
    rows = c.execute(
        "select name,file,line from symbols where kind='class' and file like ? order by file,line limit 12",
        (pat,)
    ).fetchall()
    for n, f, ln in rows:
        print(f'  {n}  |  {f}:{ln}')

show('Character', '%/Character/%')
show('Camera', '%/Camera/%')
show('Input', '%/Input/%')
show('Player', '%/Player/%')
