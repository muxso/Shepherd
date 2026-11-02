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
if __name__=='__main__':unittest.main()
