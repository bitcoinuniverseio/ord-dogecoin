"""Conservative launch-only resource gate. Never stops an admitted worker."""
import math


def calm(sample, now):
    def number(v):
        return type(v) in (int, float) and math.isfinite(v)
    values = [sample.get(k) for k in ('checked_unix', 'io_full_avg10', 'io_some_avg10',
                                     'memory_available_bytes', 'free_host_bytes')]
    if not all(number(x) for x in values) or not number(now):
        return False
    checked, full, some, memory, free = values
    return (-1 <= now-checked <= 15 and 0 <= full <= 5 and 0 <= some <= 10
            and full <= some and memory >= 24*1024**3
            and free >= 300_000_000_000 + 256*1024**3)


class CalmWindow:
    def __init__(self):
        self.samples = []

    def observe(self, sample, now):
        if not calm(sample, now):
            self.samples = []
            return False
        checked = sample['checked_unix']
        if self.samples and (checked < self.samples[-1] or checked-self.samples[-1] > 20):
            self.samples = []
            return False
        if not self.samples or checked-self.samples[-1] >= 10:
            self.samples.append(checked)
            self.samples = self.samples[-3:]
        return len(self.samples) == 3 and self.samples[-1]-self.samples[0] >= 20
