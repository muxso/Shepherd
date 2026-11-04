import datetime as dt
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from .index import Indexer,git,eligible,sanitize
from .git_activity import fetch,collect,daily_documents
from .yunxiao_documents import YunxiaoDocuments,document,rich_text

class KnowledgeTests(unittest.TestCase):
    def test_requirement_jsonml_and_case_steps_are_searchable_and_redacted(self):
        c={'projectId':'p','yunxiao':{'sourceId':'s'}}
        raw=json.dumps({'format':'jsonML','content':json.dumps(['article',{},['h1',{},'支付申请'],['p',{},'负数不可提交']])})
        self.assertIn('负数不可提交',rich_text(raw))
        self.assertEqual(rich_text(json.dumps({'htmlValue':'<p>真实正文</p>','jsonMLValue':['p',{},'真实正文']})),'真实正文')
        self.assertIn('真实正文',rich_text({'jsonMLValue':['p',{},'真实正文']}))
        self.assertIn('图片',rich_text(['img',{'src':'private image'}]))
        self.assertNotIn('evil',rich_text('<p>正文</p><script>evil()</script>'))
        req=document(c,'requirement',{'externalId':'r','serial':'REQ-1','title':'支付','description':raw})
        case=document(c,'testcase',{'externalId':'case','title':'负数校验','precondition':'已登录','steps':[{'step':'输入负数','expected':'提示错误'}]})
        self.assertIn('负数不可提交',req['text']);self.assertIn('输入负数',case['text']);self.assertIn('提示错误',case['text'])
        self.assertNotEqual(req['documentId'],case['documentId']);self.assertEqual(case['visibilityGroups'],[])
        redacted=document(c,'requirement',{'externalId':'r','description':'password=do-not-index'})
        self.assertNotIn('do-not-index',redacted['text'])
        self.assertIn('不能推断测试覆盖',document(c,'testcase',{'externalId':'empty'})['text'])
    def test_snapshot_and_incremental_retirement_only_after_complete_source(self):
        with tempfile.TemporaryDirectory() as root:
            class Fake(YunxiaoDocuments):
                revision='v1';removed=False;change=False;fail=False;calls=[]
                def api(self,method,path,body=None):
                    if path=='/api/devops/sources': return {'items':[{'id':'s','project_id':'p','last_success_at':self.revision}]}
                    if '/records?' in path:
                        items=[] if self.removed else [{'externalId':'r','serial':'R1','description':'支付约束','steps':[{'step':'操作','expected':'成功'}]}]
                        if self.change:self.revision='v2'
                        return {'total':len(items),'items':items}
                    self.calls.append((method,path,body))
                    if self.fail:raise RuntimeError('embedding unavailable')
                    return {'embedded':True,'chunks':1}
            worker=Fake({'stateDir':root,'apiUrl':'http://test','projectId':'p','embeddingRevision':'v1','yunxiao':{'sourceId':'s'}})
            worker.run();count=len(worker.calls);worker.run();self.assertEqual(len(worker.calls),count)
            worker.change=True
            with self.assertRaises(RuntimeError):worker.run()
            self.assertFalse(any(c[0]=='DELETE' for c in worker.calls))
            worker.change=False;worker.removed=True;worker.run()
            self.assertEqual(sum(c[0]=='DELETE' for c in worker.calls),2)
    def test_all_remote_branches_deduplicate_and_exclude_merges(self):
        with tempfile.TemporaryDirectory() as tmp:
            source=Path(tmp)/'source';bare=Path(tmp)/'bare'
            subprocess.run(['git','init','-q','-b','main',str(source)],check=True)
            def run(*args):return subprocess.check_output(['git','-C',str(source),*args],stderr=subprocess.PIPE)
            def commit(file,author):
                (source/file).write_text('line\n');run('add','-A');run('-c','user.name='+author,'-c','user.email='+author+'@example.invalid','commit','-qm','修改 '+file)
            commit('a.rs','Alice');run('checkout','-qb','feature');commit('b.rs','Bob');run('checkout','-q','main');commit('c.rs','Alice')
            run('-c','user.name=Alice','-c','user.email=Alice@example.invalid','merge','--no-ff','-m','merge feature','feature')
            subprocess.run(['git','clone','--bare','--depth=1','--single-branch','--branch','main',source.as_uri(),str(bare)],check=True,stderr=subprocess.PIPE)
            spec={'id':'r','name':'org/repo','branch':'main'};config={'gitHistoryDays':90,'projectId':'p'}
            fetch(bare,spec,config,git);facts=collect(bare,spec,config,git,eligible,sanitize)
            self.assertEqual(len(facts),3)
            self.assertEqual(sum(c['additions'] for c in facts),3)
            self.assertEqual(len({c['sha'] for c in facts}),3)
            docs=daily_documents(config,facts+facts)
            self.assertEqual(len(docs),2)
            self.assertEqual(sum(json.loads(d['sourceId'])['commits'] for d in docs),3)
            self.assertTrue(all('不等于全部工作或工时' in d['text'] for d in docs))
            self.assertTrue(all(d['visibilityGroups']==[] for d in docs))
            self.assertTrue(any('修改 b.rs' in d['text'] for d in docs))
