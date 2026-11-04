#!/usr/bin/env python3
"""Local checkpoints only; never print credentials or source document bodies."""
import argparse
import hashlib
import json
from pathlib import Path

def read(path):
    try:return json.loads(Path(path).read_text())
    except FileNotFoundError:return {}

def report(config,cloud=None):
    c=read(config);root=Path(c['stateDir']);repos=read(c['repositoriesFile']);done=files=chunks=0
    for repo in repos:
        key=hashlib.sha256(str(repo['id']).encode()).hexdigest()[:24];state=read(root/(key+'.json'))
        done+=state.get('status')=='complete';files+=len(state.get('files',{}));chunks+=sum(f.get('chunks',0) for f in state.get('files',{}).values())
    summary=read(root/'summary.json');activity=read(root/'git-activity.json')
    out={'repositories':{'complete':done,'total':len(repos),'lastRun':summary},'codeFiles':files,'codeChunks':chunks,
         'gitActivity':{k:v for k,v in activity.items() if k not in ('identity','documents')}}
    if cloud and Path(cloud).exists():
        cc=read(cloud);s=read(Path(cc['stateDir'])/'yunxiao-documents.json')
        out['yunxiao']={k:v for k,v in s.items() if k not in ('identity','documents')}
        out['yunxiao']['sourceScan']=read(Path(cc['stateDir'])/'yunxiao-source-scan.json')
        out['yunxiao']['byType']={kind:sum(v.get('sourceType')==kind for v in s.get('documents',{}).values()) for kind in ('yunxiao_requirement','yunxiao_testcase')}
    return out
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--config',default=str(Path.home()/'.config/shepherd-code-rag/config.json'));p.add_argument('--yunxiao-config',default=str(Path.home()/'.config/shepherd-code-rag/yunxiao.json'));a=p.parse_args()
    r=report(a.config,a.yunxiao_config)
    print(f"代码仓库: {r['repositories']['complete']}/{r['repositories']['total']}；已入库文件: {r['codeFiles']:,}；代码知识块: {r['codeChunks']:,}")
    activity=r['gitActivity'];print(f"Git 日活动: {activity.get('status','未开始')}；已入库作者日记录: {activity.get('indexedDays',len(activity.get('documents',{})))}")
    if 'yunxiao' in r:
        y=r['yunxiao'];print(f"云效文档: {y.get('status','未开始')}；{y.get('indexedDocuments',0)}/{y.get('total','?')}；需求 {y['byType']['yunxiao_requirement']}，测试用例 {y['byType']['yunxiao_testcase']}")
    print('状态来自本机检查点；定时任务和实时日志请查看 systemctl --user / journalctl --user。')
