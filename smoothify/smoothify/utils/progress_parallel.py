from joblib import Parallel
from tqdm import tqdm


class ProgressParallel(Parallel):
    def __call__(self, iterable, *args, **kwargs):
        try:
            num_tasks = len(iterable)
        except TypeError:
            num_tasks = None
        self._pbar = tqdm(total=num_tasks)
        with self._pbar:
            return super().__call__(iterable, *args, **kwargs)

    def print_progress(self):
        self._pbar.total = self.n_dispatched_tasks
        self._pbar.update(self.n_completed_tasks - self._pbar.n)
