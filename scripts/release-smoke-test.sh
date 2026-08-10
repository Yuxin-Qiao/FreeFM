#!/bin/sh
set -eu

echo "=== FreeFM Release Smoke Test ==="

# 1. Verify shell syntax of installer and helper scripts
echo "[1/4] Validating shell script syntax..."
sh -n scripts/install.sh
sh -n skills/freefm/scripts/freefm-sync.sh
sh -n scripts/package-workbuddy.sh
echo "  ✓ Script syntax valid."

# 2. Build release binary if needed
echo "[2/4] Ensuring release binary exists..."
if [ ! -f "target/release/freefm" ]; then
  cargo build --release --locked
fi
echo "  ✓ Release binary ready."

# 3. Test installer fail-closed behavior & full installation pipeline via python mock runner
echo "[3/4] Testing installer fail-closed security and installation pipeline..."
python3 -c '
import os, subprocess, tempfile, shutil

tmp = tempfile.mkdtemp()
bin_path = os.path.abspath("target/release/freefm")

tag = "v0.1.0"
arch = "arm64" if os.uname().machine == "arm64" else "x86_64"
os_name = "darwin" if os.uname().sysname == "Darwin" else "linux"
art_name = f"freefm-{tag}-{os_name}-{arch}"
art_dir = os.path.join(tmp, art_name)
os.makedirs(art_dir)
shutil.copy(bin_path, os.path.join(art_dir, "freefm"))

tarball = os.path.join(tmp, f"{art_name}.tar.gz")
subprocess.check_call(["tar", "-C", tmp, "-czf", tarball, art_name])

sha_file = f"{tarball}.sha256"
out = subprocess.check_output(["shasum", "-a", "256", tarball]).decode()
with open(sha_file, "w") as f:
    f.write(out)

# Create mock curl that copies local files
fake_bin = os.path.join(tmp, "fake_bin")
os.makedirs(fake_bin)
fake_curl = os.path.join(fake_bin, "curl")
with open(fake_curl, "w") as f:
    f.write(f"""#!/bin/sh
url="$2"
out="$4"
filename=$(basename "$url")
src="{tmp}/$filename"
if [ -f "$src" ]; then
  cp "$src" "$out"
  exit 0
else
  echo "404 not found" >&2
  exit 22
fi
""")
os.chmod(fake_curl, 0o755)

base_env = os.environ.copy()
old_path = base_env.get("PATH", "")
base_env["PATH"] = f"{fake_bin}:{old_path}"
base_env["FREEFM_VERSION"] = tag

# Test Case A: Valid artifact and valid checksum -> SUCCESS
target_a = os.path.join(tmp, "install_a")
env_a = base_env.copy()
env_a["FREEFM_INSTALL_DIR"] = target_a
res_a = subprocess.run(["sh", "scripts/install.sh"], env=env_a, capture_output=True, text=True)
assert res_a.returncode == 0, f"Case A failed: {res_a.stderr}"
installed_bin_a = os.path.join(target_a, "freefm")
assert os.path.exists(installed_bin_a), "Case A: installed binary missing"
ver_a = subprocess.check_output([installed_bin_a, "--version"]).decode().strip()
print(f"  ✓ Case A passed: clean install and version check ({ver_a})")

# Test Case B: Checksum mismatch -> FAIL-CLOSED (exit 1)
with open(sha_file, "w") as f:
    f.write("0000000000000000000000000000000000000000000000000000000000000000  freefm.tar.gz\n")

target_b = os.path.join(tmp, "install_b")
env_b = base_env.copy()
env_b["FREEFM_INSTALL_DIR"] = target_b
res_b = subprocess.run(["sh", "scripts/install.sh"], env=env_b, capture_output=True, text=True)
assert res_b.returncode == 1, "Case B: expected exit code 1 on checksum mismatch"
assert "Checksum mismatch" in res_b.stderr or "Refusing to install" in res_b.stderr, "Case B stderr missing fail-closed message"
assert not os.path.exists(os.path.join(target_b, "freefm")), "Case B: binary should NOT be installed on mismatch"
print("  ✓ Case B passed: checksum mismatch rejected (fail-closed)")

# Restore valid checksum
with open(sha_file, "w") as f:
    f.write(out)

# Test Case C: Missing checksum file (curl failure) -> FAIL-CLOSED (exit 1)
os.remove(sha_file)
target_c = os.path.join(tmp, "install_c")
env_c = base_env.copy()
env_c["FREEFM_INSTALL_DIR"] = target_c
res_c = subprocess.run(["sh", "scripts/install.sh"], env=env_c, capture_output=True, text=True)
assert res_c.returncode == 1, "Case C: expected exit code 1 on missing checksum"
assert "Failed to download checksum file" in res_c.stderr, "Case C stderr missing expected error message"
assert not os.path.exists(os.path.join(target_c, "freefm")), "Case C: binary should NOT be installed on missing checksum"
print("  ✓ Case C passed: missing checksum rejected (fail-closed)")

# Re-create valid checksum file for Case D
with open(sha_file, "w") as f:
    f.write(out)

# Test Case D: Missing sha256sum/shasum tools -> FAIL-CLOSED (exit 1)
target_d = os.path.join(tmp, "install_d")
env_d = base_env.copy()
env_d["FREEFM_INSTALL_DIR"] = target_d
strip_bin = os.path.join(tmp, "strip_bin")
os.makedirs(strip_bin)
os.symlink(fake_curl, os.path.join(strip_bin, "curl"))
for cmd in ["sh", "uname", "mktemp", "rm", "tar", "mkdir", "cp", "chmod", "awk", "grep", "sed", "tr", "find", "head", "cat", "basename"]:
    which = shutil.which(cmd)
    if which and not os.path.exists(os.path.join(strip_bin, cmd)):
        try:
            os.symlink(which, os.path.join(strip_bin, cmd))
        except FileExistsError:
            pass

# Create dummy non-executable sha256sum and shasum scripts or broken commands
for hash_cmd in ["sha256sum", "shasum"]:
    fake_hash = os.path.join(strip_bin, hash_cmd)
    with open(fake_hash, "w") as f:
        f.write("#!/bin/sh\nexit 127\n")
    os.chmod(fake_hash, 0o644) # non-executable so command -v ignores them

env_d["PATH"] = strip_bin
res_d = subprocess.run(["sh", "scripts/install.sh"], env=env_d, capture_output=True, text=True)
assert res_d.returncode == 1, f"Case D expected exit code 1, got {res_d.returncode}. Stderr: {res_d.stderr}"
assert "Neither sha256sum nor shasum is available" in res_d.stderr, f"Case D stderr missing tool error: {res_d.stderr}"
assert not os.path.exists(os.path.join(target_d, "freefm")), "Case D: binary should NOT be installed when tool is missing"
print("  ✓ Case D passed: missing checksum tool rejected (fail-closed)")

shutil.rmtree(tmp)
'
echo "  ✓ All 4 installer fail-closed & smoke test scenarios passed."

# 4. Verify WorkBuddy package generation
echo "[4/4] Verifying WorkBuddy release artifact packaging..."
scripts/package-workbuddy.sh target/freefm-workbuddy.zip >/dev/null
unzip -l target/freefm-workbuddy.zip | grep -q 'freefm/SKILL.md'
unzip -l target/freefm-workbuddy.zip | grep -q 'freefm/scripts/freefm-sync.sh'
echo "  ✓ WorkBuddy package verified."

echo ""
echo "=== RELEASE SMOKE TEST PASSED SUCCESSFULLY ==="
