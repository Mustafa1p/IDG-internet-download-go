"""Move only an isolated preview process between the actual Windows monitors."""
import ctypes, json, os, subprocess, time, sys
from ctypes import wintypes as w
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/"target/monitor-qa"; OUT.mkdir(parents=True,exist_ok=True)
u=ctypes.WinDLL("user32",use_last_error=True)
u.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
u.GetWindowThreadProcessId.argtypes=[w.HWND,ctypes.POINTER(w.DWORD)]
u.IsWindowVisible.argtypes=[w.HWND]
u.GetWindowTextW.argtypes=[w.HWND,w.LPWSTR,ctypes.c_int]
u.GetClientRect.argtypes=[w.HWND,ctypes.POINTER(w.RECT)]
u.GetWindowRect.argtypes=[w.HWND,ctypes.POINTER(w.RECT)]
u.GetDpiForWindow.argtypes=[w.HWND]; u.GetDpiForWindow.restype=w.UINT
u.SetWindowPos.argtypes=[w.HWND,w.HWND,ctypes.c_int,ctypes.c_int,ctypes.c_int,ctypes.c_int,w.UINT]
u.GetWindowDpiAwarenessContext.argtypes=[w.HWND];u.GetWindowDpiAwarenessContext.restype=ctypes.c_void_p
u.AreDpiAwarenessContextsEqual.argtypes=[ctypes.c_void_p,ctypes.c_void_p]
u.PostMessageW.argtypes=[w.HWND,w.UINT,w.WPARAM,w.LPARAM]
class Monitor(ctypes.Structure):
    _fields_=[("size",w.DWORD),("rect",w.RECT),("work",w.RECT),("flags",w.DWORD),("device",w.WCHAR*32)]
MCB=ctypes.WINFUNCTYPE(w.BOOL,w.HMONITOR,w.HDC,ctypes.POINTER(w.RECT),w.LPARAM)
monitors=[]
@MCB
def collect(handle,dc,rect,data):
    info=Monitor(); info.size=ctypes.sizeof(info); u.GetMonitorInfoW(handle,ctypes.byref(info))
    monitors.append({"device":info.device,"primary":bool(info.flags&1),"work":[info.work.left,info.work.top,info.work.right,info.work.bottom]})
    return True
u.EnumDisplayMonitors(None,None,collect,0)
monitors.sort(key=lambda m:not m["primary"])
assert len(monitors)>=2,"At least two connected monitors required"
report=OUT/"live.json"; report.unlink(missing_ok=True)
proc=subprocess.Popen([os.environ.get("IDG_UI_BINARY",str(ROOT/"target/debug/IDG.exe")),("--settings-preview" if "--settings" in sys.argv else "--ui-preview"),"--ui-preview-hold","--preview-native-dpi","--preview-populated","--preview-long-host","--preview-settings-tab=3","--settings-preview-size=960x640",f"--preview-monitor-report={report}"],cwd=ROOT,creationflags=subprocess.CREATE_NO_WINDOW)
WCB=ctypes.WINFUNCTYPE(w.BOOL,w.HWND,w.LPARAM)
hwnds=[]
@WCB
def window(handle,data):
    pid=w.DWORD();u.GetWindowThreadProcessId(handle,ctypes.byref(pid))
    title=ctypes.create_unicode_buffer(256);u.GetWindowTextW(handle,title,256)
    if pid.value==proc.pid and u.IsWindowVisible(handle) and title.value=="IDG Settings Preview":hwnds.append(handle)
    return True
checks=[]
try:
    deadline=time.monotonic()+15
    while not hwnds:
        u.EnumWindows(window,0)
        assert time.monotonic()<deadline,"Preview window did not appear"
        time.sleep(.05)
    hwnd=hwnds[0]
    print("Test HWND:",hwnd,flush=True)
    print("Awareness:",[(v,bool(u.AreDpiAwarenessContextsEqual(u.GetWindowDpiAwarenessContext(hwnd),ctypes.c_void_p(v)))) for v in [-1,-2,-3,-4]],flush=True)
    time.sleep(.5)
    def sample(monitor, expected, label):
        deadline=time.monotonic()+5
        stable=0
        while time.monotonic()<deadline:
            rect=w.RECT();assert u.GetClientRect(hwnd,ctypes.byref(rect))
            try:data=json.loads(report.read_text())
            except (OSError,json.JSONDecodeError):time.sleep(.05);continue
            dpi=u.GetDpiForWindow(hwnd)
            valid=abs(data["pixels_per_point"]-dpi/96)<.02 and abs(data["zoom"]-1)<.01
            valid=valid and abs(rect.right-data["logical_size"][0]*dpi/96)<3 and abs(rect.bottom-data["logical_size"][1]*dpi/96)<3
            if expected is not None:valid=valid and all(abs(data["logical_size"][i]-expected[i])<3 for i in [0,1])
            stable=stable+1 if valid else 0
            if stable>=4:break
            time.sleep(.1)
        assert stable>=4,(label,data,dpi,[rect.right,rect.bottom])
        data.update({"label":label,"monitor":monitor["device"],"native_dpi":dpi,"physical_size":[rect.right,rect.bottom]})
        for name,bounds in data["controls"].items():
            assert bounds[0]>=0 and bounds[2]<=data["logical_size"][0]+1,(label,name,data)
        if "--settings" in sys.argv:
            assert data["controls"]["Settings section"][3]<data["controls"]["Save changes"][1],data
        checks.append(data)
        print(f"{label}: native DPI {dpi}, client {rect.right}x{rect.bottom}, logical {data['logical_size']}: PASS",flush=True)
        try:
            from PIL import ImageGrab
            outer=w.RECT();u.GetWindowRect(hwnd,ctypes.byref(outer))
            ImageGrab.grab(bbox=(outer.left,outer.top,outer.right,outer.bottom),all_screens=True).save(OUT/f"{label}.png")
        except ImportError:pass
    def move(monitor,expected,label):
        x,y,right,bottom=monitor["work"]
        assert u.SetWindowPos(hwnd,None,x+80,y+80,0,0,0x0001|0x0004|0x0010)
        # A portrait/small destination legitimately caps the client to its work
        # area. Verify that cap rather than expecting an off-screen window.
        if expected is not None:
            dpi=u.GetDpiForWindow(hwnd)/96
            expected=[min(expected[0],(right-x)/dpi),min(expected[1],(bottom-y)/dpi)]
        sample(monitor,expected,label)
    def resize(monitor,expected,label):
        dpi=u.GetDpiForWindow(hwnd)/96
        assert u.SetWindowPos(hwnd,None,0,0,round(expected[0]*dpi),round(expected[1]*dpi),0x0002|0x0004|0x0010)
        sample(monitor,expected,label)
    route=[monitors[0],*monitors[1:],monitors[0],*monitors[1:],monitors[0]]
    for index,monitor in enumerate(route):move(monitor,[960,640],f"move-{index}")
    resize(monitors[0],[1100,720],"resize-wide")
    move(monitors[1],[1100,720],"wide-secondary")
    resize(monitors[1],[800,500],"resize-minimum")
    move(monitors[-1],[800,500],"minimum-third")
    move(monitors[0],[800,500],"minimum-primary")
    u.ShowWindow.argtypes=[w.HWND,ctypes.c_int]
    u.ShowWindow(hwnd,3);time.sleep(.3);sample(monitors[0],None,"maximized")
    u.ShowWindow(hwnd,9);time.sleep(.3);sample(monitors[0],[800,500],"restored")
    (OUT/("settings-results.json" if "--settings" in sys.argv else "results.json")).write_text(json.dumps(checks,indent=2))
    print("Native monitor transitions, resize and restore preserved logical size and visible controls: PASS",flush=True)

finally:
    if hwnds:u.PostMessageW(hwnds[0],0x0010,0,0)
    try:proc.wait(timeout=5)
    except subprocess.TimeoutExpired:proc.terminate();proc.wait(timeout=5)
