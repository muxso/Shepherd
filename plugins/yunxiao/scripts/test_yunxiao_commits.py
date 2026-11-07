import datetime as dt
import types
import unittest
from yunxiao_commits import list_commits
class PaginationTests(unittest.TestCase):
 def run_pages(self,pages,max_pages=1000):
  calls=[]
  def api(path,query):calls.append(query['page']);return pages[min(query['page']-1,len(pages)-1)],{'X-Total-Pages':'1','X-Total':'100'}
  collector=types.SimpleNamespace(api=api,rows=lambda data:data);now=dt.datetime.now(dt.timezone.utc)
  return list_commits(collector,{'id':'repo'},'feature',now-dt.timedelta(days=90),now,{'YUNXIAO_ORG':'org'},max_pages),calls
 def test_full_and_short_pages_continue_despite_incorrect_total_headers(self):
  rows,calls=self.run_pages([[{'id':'a'}],[{'id':'b'}],[]]);self.assertEqual([r['id'] for r in rows],['a','b']);self.assertEqual(calls,[1,2,3])
 def test_overlapping_commits_are_deduplicated(self):
  rows,_=self.run_pages([[{'id':'a'},{'id':'b'}],[{'id':'b'},{'id':'c'}],[]]);self.assertEqual([r['id'] for r in rows],['a','b','c'])
 def test_repeated_pages_and_exhausted_limits_fail_closed(self):
  with self.assertRaises(RuntimeError):self.run_pages([[{'id':'a'}]])
  with self.assertRaises(RuntimeError):self.run_pages([[{'id':'a'}],[{'id':'b'}],[]],max_pages=2)
if __name__=='__main__':unittest.main()
