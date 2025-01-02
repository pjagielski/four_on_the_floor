import time
from watchdog.observers import Observer
from watchdog.events import FileSystemEventHandler
import subprocess

class FileChangeHandler(FileSystemEventHandler):
    def __init__(self, path_to_watch, script_to_run):
        self.path_to_watch = path_to_watch
        self.script_to_run = script_to_run

    def on_modified(self, event):
        if event.src_path.endswith("live.py"):
            print(f"{event.src_path} modified. Re-running script...")
            subprocess.run(["python", f"{self.path_to_watch}/{self.script_to_run}"], check=True)

if __name__ == "__main__":
    path_to_watch = "src"
    script_to_run = "live.py"

    event_handler = FileChangeHandler(path_to_watch, script_to_run)
    observer = Observer()
    observer.schedule(event_handler, path=path_to_watch, recursive=False)

    print(f"Watching {path_to_watch} for changes...")
    observer.start()

    try:
        while True:
            time.sleep(1)
    except KeyboardInterrupt:
        observer.stop()
    observer.join()
