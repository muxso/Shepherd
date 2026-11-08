import unittest,types,datetime as dt
import yunxiao_sync as sync
class SyncTests(unittest.TestCase):
 def test_batches_respect_both_limits(self):
  records=[{'data':'汉'*100,'externalId':str(i)} for i in range(250)]
  batches=list(sync.batches(records,2000));self.assertEqual(sum(map(len,batches)),250)
  self.assertTrue(all(len(b)<=100 for b in batches))
 def test_duplicates_use_latest_source_update_and_preserve_kind(self):
  a={'kind':'bug','externalId':'1','data':{'title':'old','updatedAt':'2026-10-27T00:00:00+08:00'}}
  b={'kind':'bug','externalId':'1','data':{'title':'new','updatedAt':'2026-10-28T00:00:00+08:00'}}
  c={'kind':'requirement','externalId':'1','data':{}}
  out=sync.deduplicate([b,a,c]);self.assertEqual(len(out),2);self.assertEqual(out[0]['data']['title'],'new')
 def test_due_priority_and_owner_remain_source_facts(self):
  def date(v):
   try:return dt.datetime.fromisoformat(v)
   except (TypeError,ValueError):return None
  c=types.SimpleNamespace(pname=lambda x:x.get('name','') if isinstance(x,dict) else str(x or ''),dateval=date)
  g=types.SimpleNamespace(canonical_name=lambda x:x,priority_name=lambda x:'低',group_for=lambda x:'产品运营一组')
  r=sync.normalize_workitem({'id':'1','subject':'测试','assignedTo':{'name':'产品'},'creator':{'name':'原始创建人'},'status':{'name':'开发中'},'customFieldValues':[{'fieldName':'计划完成日期','values':[{'identifier':'2026-10-01'}]}]},'requirement',c,g,{})
  self.assertEqual(r['data']['due'],'2026-10-01');self.assertEqual(r['data']['priority'],'低')
  self.assertNotIn('author',r['data'])
  self.assertEqual(r['data']['creator'],'原始创建人')
  self.assertEqual(r['data']['owner'],'产品')
  history={'complete':True,'checkedAt':'2026-11-02T00:00:00+08:00','events':[{'id':'1','at':'2026-10-20T00:00:00+08:00'}]}
  bug=sync.normalize_workitem({'id':'bug','reopenHistory':history},'bug',c,g,{})
  self.assertEqual(bug['data']['reopenHistory'],history)

class BranchTrackingTests(unittest.TestCase):
 def test_shared_commits_keep_every_scanned_branch_and_restore_collector(self):
  from unittest.mock import patch
  now=dt.datetime.now(dt.timezone(dt.timedelta(hours=8))).isoformat()
  commit={'id':'sha','authorName':'GitAuthor','authoredDate':now}
  original=lambda *args:[commit]
  collector=types.SimpleNamespace(cfg=lambda:{},workitems=lambda *args:[],sprints=lambda *args:{},flow_ci=lambda *args:{},commits=original,TZ=dt.timezone(dt.timedelta(hours=8)))
  def report(a,b,allow):
   collector.commits({'id':'repo'},'feature/a',a,b,{})
   collector.commits({'id':'repo'},'feature/b',a,b,{})
   return {'totals':{},'commitsDetail':[{'repo_id':'repo','repo':'service','sha':'sha','author':'GitAuthor','date':now,'stats':{'additions':1,'deletions':0},'requirementIds':[]}]}
  collector.report=report
  groups=types.SimpleNamespace(canonical_name=lambda s:s,group_for=lambda s:'Team')
  with patch.object(sync,'enrich_requirements',lambda rows,*a:rows),patch.object(sync,'enrich_bug_relations',lambda rows,*a:rows),patch.object(sync,'enrich_bug_history',lambda rows,*a:rows),patch.object(sync,'collect_testcases',lambda *a:[]),patch.object(sync,'repository_language',lambda *a:{}):
   records,_,_=sync.collect(collector,groups,7)
  self.assertEqual(records[0]['data']['branches'],['feature/a','feature/b'])
  self.assertIs(collector.commits,original)

if __name__=='__main__':unittest.main()
