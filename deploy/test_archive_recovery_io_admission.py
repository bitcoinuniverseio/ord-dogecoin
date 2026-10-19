import unittest
from archive_recovery_io_admission import calm, CalmWindow


def sample(t):
    return dict(checked_unix=t,io_full_avg10=5,io_some_avg10=10,
                memory_available_bytes=24*1024**3,free_host_bytes=300_000_000_000+256*1024**3)


class IOAdmission(unittest.TestCase):
    def test_three_spaced_calm_samples_required(self):
        gate=CalmWindow()
        for t, expected in [(1000,False),(1001,False),(1010,False),(1020,True)]:
            self.assertEqual(gate.observe(sample(t),t),expected)

    def test_pressure_or_resource_failure_resets_window(self):
        for key,value in [('io_full_avg10',5.01),('io_some_avg10',10.01),
                          ('memory_available_bytes',24*1024**3-1),('free_host_bytes',0)]:
            gate=CalmWindow();gate.observe(sample(1000),1000);bad=sample(1010);bad[key]=value
            self.assertFalse(gate.observe(bad,1010));self.assertEqual(gate.samples,[])

    def test_stale_nonfinite_boolean_and_impossible_pressure_refused(self):
        for key,value in [('checked_unix',980),('io_full_avg10',float('nan')),
                          ('free_host_bytes',True),('io_full_avg10',11),('io_some_avg10',-1)]:
            s=sample(1000);s[key]=value;self.assertFalse(calm(s,1000))

    def test_clock_reversal_resets_window(self):
        gate=CalmWindow();gate.observe(sample(1000),1000)
        self.assertFalse(gate.observe(sample(999),999));self.assertEqual(gate.samples,[])

    def test_long_gap_does_not_count_as_sustained_calm(self):
        gate=CalmWindow();gate.observe(sample(1000),1000);gate.observe(sample(1010),1010)
        self.assertFalse(gate.observe(sample(1040),1040));self.assertEqual(gate.samples,[])


if __name__=='__main__':unittest.main()
