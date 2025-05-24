import subprocess
import sys
from rich.console import Console
from rich.panel import Panel

def run_cli(*args):
    result = subprocess.run(
        ["soon", *args],
        capture_output=True,
        text=True
    )
    return result

def main():
    args = sys.argv[1:]
    result = run_cli(*args)
    console = Console()
    if result.returncode == 0:
        console.print(Panel(result.stdout.strip(), title="Result"))
    else:
        console.print(Panel(result.stderr.strip() or "Unknown error", title="Error", style="red"))
    return result.returncode

if __name__ == "__main__":
    sys.exit(main())