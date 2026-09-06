import unittest
import sys
import tempfile
import shutil
from pathlib import Path

# Add execution directory to sys.path
sys.path.append(str(Path(__file__).parent.parent.parent / "execution"))

import verify_ai_context

class TestVerifyAiContextFix(unittest.TestCase):
    def setUp(self):
        self.test_dir = tempfile.mkdtemp()

    def tearDown(self):
        shutil.rmtree(self.test_dir)

    def test_fix_python_file(self):
        file_path = Path(self.test_dir) / "test_module.py"
        content = "def test_func():\n    pass\n"
        
        fixed = verify_ai_context.fix_file_context(file_path, content)
        
        self.assertIn("### AI Assist Note", fixed)
        self.assertIn("### 🔍 Debugging & Observability", fixed)
        self.assertIn("test_func()", fixed)

    def test_fix_rust_file(self):
        file_path = Path(self.test_dir) / "test_module.rs"
        content = "pub fn test_rust() {}"
        
        fixed = verify_ai_context.fix_file_context(file_path, content)
        
        self.assertTrue(fixed.startswith("//! @docs ARCHITECTURE:Core"))
        self.assertIn("### AI Assist Note", fixed)
        self.assertIn("pub fn test_rust()", fixed)

    def test_fix_typescript_file(self):
        file_path = Path(self.test_dir) / "test_module.ts"
        content = "export const test = () => {};"
        
        fixed = verify_ai_context.fix_file_context(file_path, content)
        
        self.assertTrue(fixed.startswith("/**"))
        self.assertIn("### AI Assist Note", fixed)
        self.assertIn("export const test", fixed)

    def test_extract_metadata(self):
        sample = """
        //! @docs ARCHITECTURE:Persistence
        //!
        //! ### AI Assist Note
        //! Core technical resource.
        //!
        //! ### 🔍 Debugging & Observability
        //! - **Telemetry Link**: Search `[test_verify_ai_context_fix]` in system logs.
        //! - Search `[Database]` in logs.
        """
        meta = verify_ai_context.extract_metadata(sample)
        self.assertTrue(meta["has_note"])
        self.assertTrue(meta["has_debugging"])
        self.assertEqual(meta["telemetry_tag"], "test_verify_ai_context_fix")

if __name__ == "__main__":
    unittest.main()





# Metadata: [test_verify_ai_context_fix]
