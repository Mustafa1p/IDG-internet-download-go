"""Windows lifetime counters for an owned subprocess, collected outside timing.

Query the still-open Popen handle after wait(), before releasing the Popen.
No polling/sampling thread is needed. Counters exclude descendant processes.
API definitions: https://learn.microsoft.com/windows/win32/api/psapi/ns-psapi-process_memory_counters
https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes
"""
import ctypes
from ctypes import wintypes
import os


def snapshot(process):
    result = {"scope": "client process only; excludes descendants and test server", "errors": []}
    if os.name != "nt":
        result["errors"].append("Lifetime resource counters are only implemented on Windows")
        return result
    # An owned handle avoids PID reuse and remains queryable after process exit.
    handle = wintypes.HANDLE(int(process._handle))
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)

    class MemoryCounters(ctypes.Structure):
        _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD),
                    *[(name, ctypes.c_size_t) for name in (
                        "PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
                        "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage",
                        "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage")]]

    memory = MemoryCounters()
    memory.cb = ctypes.sizeof(memory)
    get_memory = kernel.K32GetProcessMemoryInfo
    get_memory.argtypes = [wintypes.HANDLE, ctypes.POINTER(MemoryCounters), wintypes.DWORD]
    get_memory.restype = wintypes.BOOL
    if get_memory(handle, ctypes.byref(memory), memory.cb):
        result.update(peak_working_set_bytes=memory.PeakWorkingSetSize,
                      peak_commit_bytes=memory.PeakPagefileUsage,
                      page_faults=memory.PageFaultCount)
    else:
        result["errors"].append(f"K32GetProcessMemoryInfo: {ctypes.WinError(ctypes.get_last_error())}")

    get_times = kernel.GetProcessTimes
    get_times.argtypes = [wintypes.HANDLE, *[ctypes.POINTER(wintypes.FILETIME)] * 4]
    get_times.restype = wintypes.BOOL
    created, exited, system, user = (wintypes.FILETIME() for _ in range(4))
    if get_times(handle, *(ctypes.byref(value) for value in (created, exited, system, user))):
        def seconds(value):
            return ((value.dwHighDateTime << 32) | value.dwLowDateTime) / 10_000_000
        result.update(cpu_user_seconds=seconds(user), cpu_kernel_seconds=seconds(system),
                      cpu_total_seconds=seconds(user) + seconds(system))
    else:
        result["errors"].append(f"GetProcessTimes: {ctypes.WinError(ctypes.get_last_error())}")
    return result
