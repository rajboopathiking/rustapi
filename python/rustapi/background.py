class BackgroundTasks:
    def __init__(self):
        self.tasks = []

    def add_task(self, func, *args, **kwargs):
        """Add a function to be executed in the background after the response."""
        self.tasks.append((func, args, kwargs))
