import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from yunxiao_repository_languages import tree_languages, repository_language

class LanguagesTest(unittest.TestCase):
    def test_tree_is_read_only_source_inventory(self):
        def entry(path,size=100,mode='100644'):
            return f'{mode} blob abc {size}\t{path}\0'.encode()
        raw=b''.join([entry('src/Main.java',300),entry('src/app.ts',200),entry('src/ui.tsx',100),entry('node_modules/a.js'),entry('dist/app.js'),entry('src/app.min.js'),entry('README.md'),entry('link.rs',1000,'120000'),entry('src/a.py',50)])
        self.assertEqual(tree_languages(raw),[{'name':'Java','bytes':300},{'name':'TypeScript','bytes':300},{'name':'Python','bytes':50}])
    def test_missing_cache_is_unknown(self):
        with tempfile.TemporaryDirectory() as root:
            self.assertEqual(repository_language('../private',root)['repositoryLanguages'],[])
    def test_git_inventory_and_reference(self):
        with tempfile.TemporaryDirectory() as root:
            (Path(root)/'repos/42').mkdir(parents=True)
            with patch('yunxiao_repository_languages.subprocess.check_output',side_effect=[b'abc\n',b'100644 blob abc 123\tsrc/main.rs\0']) as git:
                data=repository_language(42,root)
                self.assertEqual(data['repositoryLanguages'],['Rust'])
                self.assertEqual(data['repositoryLanguageCommit'],'abc')
                self.assertEqual(git.call_args.kwargs['env']['GIT_OPTIONAL_LOCKS'],'0')
if __name__=='__main__': unittest.main()
