#!/usr/bin/env python3
"""Generate domain protobuf goldens with protoc (independent wire encoder)."""
import argparse,json,re,subprocess,shutil
from pathlib import Path
root=Path(__file__).resolve().parents[1]
p=argparse.ArgumentParser();p.add_argument('--reaver',type=Path,default=root.parent/'reaver');a=p.parse_args()
schema=a.reaver/'extras/mounts/goo/protocol'
subprocess.run(['python3',str(schema/'generate.py')],check=True)
shutil.copyfile(schema/'wire_schema.rs',root/'src/wire_schema.rs')
s=re.sub(r'//[^\n]*','',(schema/'goo.proto').read_text())
messages={name:re.findall(r'(?:(required|optional|repeated)\s+)?(\w+)\s+(\w+)\s*=\s*(\d+)\s*;',body) for name,body in re.findall(r'message\s+(\w+)\s*\{((?:[^{}]|\{[^{}]*\})*)\}',s)}
def source(name,value):
    if name in ['TextRuns','RunFragments','Description']:value={'values':value}
    if name=='Myth':value={'entries':[{'path':k,'pail':v} for k,v in value.items()]}
    if name=='Pail':
        kind=value['kind'];v=value['value'];key={'bool':'boolean','unsupported':'unsupportedBrand'}.get(kind,kind)
        if kind=='modifiers':v={'values':v}
        if kind=='unsupported':v=v['brand']
        value={key:v}
    out=[]
    for mode,typ,key,tag in messages[name]:
        if value.get(key) is None:
            if mode=='required':raise ValueError((name,key))
            continue
        values=value[key] if mode=='repeated' else [value[key]]
        for item in values:
            if typ in messages:out.append(key+' { '+source(typ,item)+' }')
            else:out.append(key+': '+(str(item).lower() if isinstance(item,bool) else json.dumps(item,ensure_ascii=True)))
    return ' '.join(out)
out=root/'tests/fixtures/protobuf';out.mkdir(exist_ok=True)
for name,path in [('rich',root/'tests/fixtures/prepared-rich-input.json'),('view',root/'tests/fixtures/live-view.json')]:
    value=json.loads(path.read_text())
    if name=='rich':
        display=value['display'];value=json.loads((root/'tests/fixtures/live-view.json').read_text());value.update(display=display,profile=display['profile'],context=display['context'])
    text='wire_version: 2 presentation { '+source('Presentation',value)+' }'
    result=subprocess.run(['protoc','-I',str(schema),'--encode=goo.wire.v2.Response','goo.proto'],input=text.encode(),capture_output=True)
    if result.returncode:raise RuntimeError(result.stderr.decode())
    (out/(name+'.pb')).write_bytes(result.stdout);(out/(name+'.json')).write_text(json.dumps(value,separators=(',',':')))
    print(name,len(result.stdout),len(path.read_bytes()))

subprocess.run([str(root/"scripts/cargo-macos.sh"),"fmt","--all"],cwd=root,check=True)
