import unittest,types,datetime as dt,tempfile,os
from unittest.mock import patch
from yunxiao_testhub import text,normalize_case,enrich_testcases
class TesthubTests(unittest.TestCase):
 def test_richtext_preserves_text_lines_and_image_references(self):
  self.assertEqual(text('<p>A &amp; B</p><p>C</p>'),'A & B\n\nC')
  self.assertEqual(text('参数 <id>'),'参数 <id>')
  self.assertIn('https://example.test/a.png',text('<img src="https://example.test/a.png"/>'))
 def test_table_and_text_steps_have_expected_and_hierarchy(self):
  c=types.SimpleNamespace(pname=lambda p:(p or {}).get('name',''),dateval=lambda v:dt.datetime.fromisoformat(v) if v else None)
  item={'id':'1','subject':'case','directory':{'id':'leaf','name':'leaf'},'testSteps':{'contentType':'TABLE','content':[{'step':'<p>登录</p>','expected':'成功'}]},'preCondition':'<p>已注册</p>'}
  r=normalize_case(item,{'testRepoIdentifier':'repo','name':'库'},{'leaf':{'name':'目录','parentId':'root'},'root':{'name':'根目录'}},c)
  self.assertEqual(r['data']['modulePath'],['云效测试库','库','根目录','目录'])
  self.assertEqual(r['data']['steps'],[{'step':'登录','expected':'成功'}]);self.assertEqual(r['data']['precondition'],'已注册')
  item['testSteps']={'contentType':'TEXT','stepContent':'<p>步骤</p>','expectedResult':'<p>预期</p>','content':[{'step':'ignored'}]}
  self.assertEqual(normalize_case(item,{'testRepoIdentifier':'repo','name':'库'},{},c)['data']['steps'],[{'step':'步骤','expected':'预期'}])
 def test_missing_list_updated_time_refreshes_details(self):
  body={'value':'first'}
  def api(*args,**kwargs):return {'id':'id','gmtModified':42,'testSteps':{'contentType':'TEXT','stepContent':body['value'],'expectedResult':'expected'}},{}
  c=types.SimpleNamespace(cfg=lambda:{'YUNXIAO_ORG':'org'},api=api,pname=lambda x:(x or {}).get('name',''),dateval=lambda x:dt.datetime.fromtimestamp(x/1000,dt.timezone.utc) if x is not None else None)
  with tempfile.TemporaryDirectory() as d,patch.dict(os.environ,{'SHEPHERD_TESTCASE_CACHE':d+'/cache.json'}):
   def record():return [{'externalId':'repo:id','data':{'repoId':'repo','raw':{'id':'id','gmtModified':None}}}]
   first=enrich_testcases(record(),c);self.assertEqual(first[0]['data']['steps'][0]['step'],'first')
   body['value']='changed'
   second=enrich_testcases(record(),c);self.assertEqual(second[0]['data']['steps'][0]['step'],'changed')
   self.assertTrue(second[0]['data']['updatedAt'].startswith('1970-01-01'))
if __name__=='__main__':unittest.main()
