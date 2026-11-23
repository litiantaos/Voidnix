// 录屏桥：ScreenCaptureKit 采集 + SCRecordingOutput 直写 MP4（macOS 15+）。
//
// 为什么用 SCRecordingOutput 而不是自己拼 SCStreamOutput + AVAssetWriter：
// Apple 在 15.0 引入 RecordingOutput，一路负责帧调度、编码、音频混流与文件收尾，
// 免去帧时间戳/关键帧/音轨 mux 的全部手工纪律；本项目的录屏是「选区 → 文件 →
// 剪辑台」的轻量闭环，不需要逐帧处理（暂停/标注水印等留待后续版本）。
//
// 线程模型：voidnix_record_start 在 Rust spawn_blocking 线程同步等待 SCK 的
// completionHandler（两段信号量），绝不占用主线程；finish 回调可能来自任意
// dispatch queue，由 Rust 侧的 C 回调自行切回事件总线（app.emit 线程安全）。

#import <AVFoundation/AVFoundation.h>
#import <CoreMedia/CoreMedia.h>
#import <Foundation/Foundation.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>

#include <math.h>
#include <mutex>
#include <string.h>
#include <unistd.h>

typedef void (*VoidnixRecordFinishCallback)(int ok, long long duration_ms, const char *error);

static VoidnixRecordFinishCallback g_finish_callback = NULL;

static void VNSetError(char *buffer, size_t length, NSString *message) {
  if (buffer == NULL || length == 0) {
    return;
  }
  const char *utf8 = message.UTF8String;
  if (utf8 == NULL || utf8[0] == '\0') {
    utf8 = "录屏失败";
  }
  strlcpy(buffer, utf8, length);
}

@interface VNRecorder : NSObject <SCStreamDelegate, SCRecordingOutputDelegate>
@property(nonatomic, strong) SCStream *stream;
@property(nonatomic, strong) SCRecordingOutput *output;
@property(nonatomic, assign) BOOL finished;
@end

static VNRecorder *g_recorder = nil;
/// 保护 g_recorder：start/stop 在 Rust 工作线程与主线程，finish 在 SCK 的
/// dispatch queue——裸全局强指针的 retain/release 跨线程并发是未定义行为。
static std::mutex g_recorder_mutex;

/// 取当前实例（调用方持有 strong 引用，使用期间不会被 finish 释放）。
static VNRecorder *VNRecorderRef(void) {
  std::lock_guard<std::mutex> lock(g_recorder_mutex);
  return g_recorder;
}

/// 仅当全局仍是该实例时清除（防旧实例的迟到回调清掉新实例）。
static void VNClearRecorderIfCurrent(VNRecorder *recorder) {
  std::lock_guard<std::mutex> lock(g_recorder_mutex);
  if (g_recorder == recorder) {
    g_recorder = nil;
  }
}

@implementation VNRecorder

- (long long)durationMs {
  if (self.output == nil) {
    return 0;
  }
  CMTime duration = self.output.recordedDuration;
  if (!CMTIME_IS_VALID(duration)) {
    return 0;
  }
  return (long long)llround(CMTimeGetSeconds(duration) * 1000.0);
}

/// 唯一收尾点：幂等（delegate / stopCapture 错误 / 看门狗可能并发到达）。
- (void)finishWithOK:(BOOL)ok error:(NSString *)error {
  @synchronized(self) {
    if (self.finished) {
      return;
    }
    self.finished = YES;
  }
  VNRecorder *keeper = self;
  long long duration = [keeper durationMs];
  SCStream *stream = keeper.stream;
  if (stream != nil) {
    // 无论成功/失败都显式停流：系统录屏指示器与采集会话绑定在 SCStream 上，仅
    // RecordingOutput 结束（尤其是从系统指示器侧停止、didStopWithError 等路径）
    // 不足以清掉指示器/采集会话；stopCapture 幂等，重复调用无害。
    [stream stopCaptureWithCompletionHandler:nil];
  }
  // stream/output 在 start 发布后只读：并发 stop 读取安全；finish 不清空它们
  //（g_recorder 清除后仅剩 stopCapture completion block 短暂强持有本对象，
  // 看门狗为弱引用，SCStream 随之释放）
  VNClearRecorderIfCurrent(keeper);
  VoidnixRecordFinishCallback callback = g_finish_callback;
  if (callback != NULL) {
    NSString *message = ok ? @"" : (error.length > 0 ? error : @"录屏失败");
    callback(ok ? 1 : 0, duration, message.UTF8String);
  }
}

- (void)recordingOutputDidStartRecording:(SCRecordingOutput *)recordingOutput {
  (void)recordingOutput;
}

/// 启动失败时彻底弃用本实例：置 finished 后停流，delegate 的 finish 回调会被
/// 守卫拦住，不向 Rust 补发一个与命令错误重复的失败事件。
- (void)abandon {
  @synchronized(self) {
    self.finished = YES;
  }
  if (self.stream != nil) {
    [self.stream stopCaptureWithCompletionHandler:nil];
  }
}

- (void)recordingOutput:(SCRecordingOutput *)recordingOutput
      didFailWithError:(NSError *)error {
  (void)recordingOutput;
  [self finishWithOK:NO error:error.localizedDescription];
}

- (void)recordingOutputDidFinishRecording:(SCRecordingOutput *)recordingOutput {
  (void)recordingOutput;
  [self finishWithOK:YES error:nil];
}

- (void)stream:(SCStream *)stream didStopWithError:(NSError *)error {
  (void)stream;
  if (error == nil) {
    return;
  }
  // 流被外部（系统录屏指示器 / 显示变化）停掉时，RecordingOutput 可能仍在异步
  // 收尾写 moov；先给 1s 窗口等 recordingOutputDidFinishRecording，等到即按成功
  // 收（文件完整），等不到才按失败上报。
  NSString *message = error.localizedDescription;
  __weak VNRecorder *weak_recorder = self;
  dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(1 * NSEC_PER_SEC)),
                 dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
                   [weak_recorder finishWithOK:NO error:message];
                 });
}

@end

extern "C" {

void voidnix_record_set_finish_callback(VoidnixRecordFinishCallback callback) {
  g_finish_callback = callback;
}

/// 启动录屏（阻塞直至 SCK 真正开始出帧）。返回 false 时 error_buffer 带中文原因。
bool voidnix_record_start(uint32_t display_id,
                          double sel_x,
                          double sel_y,
                          double sel_w,
                          double sel_h,
                          double scale,
                          int fps,
                          bool system_audio,
                          bool microphone,
                          const char *path,
                          char *error_buffer,
                          size_t error_buffer_len) {
  @autoreleasepool {
    if (VNRecorderRef() != nil) {
      VNSetError(error_buffer, error_buffer_len, @"录屏已在进行中");
      return false;
    }
    if (path == NULL) {
      VNSetError(error_buffer, error_buffer_len, @"录屏输出路径无效");
      return false;
    }
    if (sel_w < 8.0 || sel_h < 8.0 || scale <= 0.0) {
      VNSetError(error_buffer, error_buffer_len, @"录制区域过小");
      return false;
    }
    if (microphone) {
      // 已拒绝/受限时 SCK 会以 -3820（FailedToStartMicrophone）失败，给可读指引；
      // 未决（NotDetermined）交给 SCK 触发系统麦克风授权弹窗
      AVAuthorizationStatus mic_status =
          [AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio];
      if (mic_status == AVAuthorizationStatusDenied ||
          mic_status == AVAuthorizationStatusRestricted) {
        VNSetError(error_buffer, error_buffer_len,
                   @"麦克风未授权：请在系统设置中允许麦克风，或关闭录屏设置中的「录制麦克风」");
        return false;
      }
    }

    NSString *file_path = [NSString stringWithUTF8String:path];
    if (file_path == nil) {
      VNSetError(error_buffer, error_buffer_len, @"录屏输出路径无效");
      return false;
    }
    NSURL *output_url = [NSURL fileURLWithPath:file_path];
    [[NSFileManager defaultManager] removeItemAtURL:output_url error:nil];

    // ① 可共享内容（含显示器与运行中应用）——SCK 的异步入口，用信号量同步成
    //    调用方语义，便于 Rust 侧把「启动成功/失败」作为命令结果返回。
    dispatch_semaphore_t content_sem = dispatch_semaphore_create(0);
    __block SCShareableContent *content = nil;
    __block NSError *content_error = nil;
    [SCShareableContent
        getShareableContentExcludingDesktopWindows:NO
                             onScreenWindowsOnly:YES
                              completionHandler:^(SCShareableContent *shareable,
                                                  NSError *error) {
                                content = shareable;
                                content_error = error;
                                dispatch_semaphore_signal(content_sem);
                              }];
    if (dispatch_semaphore_wait(
            content_sem, dispatch_time(DISPATCH_TIME_NOW, (int64_t)(10 * NSEC_PER_SEC))) != 0) {
      VNSetError(error_buffer, error_buffer_len, @"获取屏幕内容超时");
      return false;
    }
    if (content == nil) {
      VNSetError(error_buffer, error_buffer_len,
                 content_error.localizedDescription ?: @"无法读取屏幕内容（请检查录屏权限）");
      return false;
    }

    SCDisplay *target_display = nil;
    for (SCDisplay *candidate in content.displays) {
      if (candidate.displayID == display_id) {
        target_display = candidate;
        break;
      }
    }
    if (target_display == nil) {
      target_display = content.displays.firstObject;
    }
    if (target_display == nil) {
      VNSetError(error_buffer, error_buffer_len, @"找不到可录制的显示器");
      return false;
    }

    // ② 排除本应用（截图 overlay 淡出、toast、菜单栏等都不进画面）。自身进程在
    //    shareableContent.applications 里按 PID 匹配；拿不到就退化为不排除。
    pid_t self_pid = getpid();
    NSMutableArray<SCRunningApplication *> *excluded = [NSMutableArray array];
    for (SCRunningApplication *application in content.applications) {
      if (application.processID == self_pid) {
        [excluded addObject:application];
        break;
      }
    }
    SCContentFilter *filter =
        [[SCContentFilter alloc] initWithDisplay:target_display
                           excludingApplications:excluded
                                exceptingWindows:@[]];

    SCStreamConfiguration *configuration = [[SCStreamConfiguration alloc] init];
    // H.264 4:2:0 要求偶数像素；sourceRect 是显示器本地 points（左上原点），
    // width/height 是输出像素——乘 backing scale 保住 Retina 原生分辨率。
    size_t pixel_w = (size_t)llround(sel_w * scale);
    size_t pixel_h = (size_t)llround(sel_h * scale);
    pixel_w &= ~(size_t)1;
    pixel_h &= ~(size_t)1;
    if (pixel_w < 16) pixel_w = 16;
    if (pixel_h < 16) pixel_h = 16;
    configuration.width = pixel_w;
    configuration.height = pixel_h;
    configuration.sourceRect = CGRectMake(sel_x, sel_y, sel_w, sel_h);
    configuration.scalesToFit = NO;
    configuration.showsCursor = YES;
    configuration.minimumFrameInterval = CMTimeMake(1, fps > 0 ? fps : 30);
    configuration.queueDepth = 8;
    configuration.capturesAudio = system_audio;
    configuration.excludesCurrentProcessAudio = YES;
    configuration.sampleRate = 48000;
    configuration.channelCount = 2;
    configuration.captureMicrophone = microphone;

    SCRecordingOutputConfiguration *recording_configuration =
        [[SCRecordingOutputConfiguration alloc] init];
    recording_configuration.outputURL = output_url;
    recording_configuration.videoCodecType = AVVideoCodecTypeH264;
    recording_configuration.outputFileType = AVFileTypeMPEG4;

    VNRecorder *recorder = [[VNRecorder alloc] init];
    SCRecordingOutput *recording_output =
        [[SCRecordingOutput alloc] initWithConfiguration:recording_configuration
                                                delegate:recorder];
    SCStream *stream = [[SCStream alloc] initWithFilter:filter
                                         configuration:configuration
                                              delegate:recorder];
    NSError *attach_error = nil;
    if (![stream addRecordingOutput:recording_output error:&attach_error]) {
      VNSetError(error_buffer, error_buffer_len,
                 attach_error.localizedDescription ?: @"添加录制输出失败");
      return false;
    }
    recorder.stream = stream;
    recorder.output = recording_output;
    {
      std::lock_guard<std::mutex> lock(g_recorder_mutex);
      g_recorder = recorder;
    }

    // ③ 真正开始采集（RecordingOutput 与 stream 同生命周期，开始即写文件）
    dispatch_semaphore_t start_sem = dispatch_semaphore_create(0);
    __block NSError *start_error = nil;
    [stream startCaptureWithCompletionHandler:^(NSError *error) {
      start_error = error;
      dispatch_semaphore_signal(start_sem);
    }];
    if (dispatch_semaphore_wait(
            start_sem, dispatch_time(DISPATCH_TIME_NOW, (int64_t)(10 * NSEC_PER_SEC))) != 0) {
      VNClearRecorderIfCurrent(recorder);
      [recorder abandon];
      [[NSFileManager defaultManager] removeItemAtURL:output_url error:nil];
      VNSetError(error_buffer, error_buffer_len, @"开始录屏超时");
      return false;
    }
    if (start_error != nil) {
      VNClearRecorderIfCurrent(recorder);
      [recorder abandon];
      [[NSFileManager defaultManager] removeItemAtURL:output_url error:nil];
      VNSetError(error_buffer, error_buffer_len,
                 start_error.localizedDescription ?: @"开始录屏失败");
      return false;
    }
    // 启动期间流若已失败并 finish（g_recorder 被清），不能报成功——否则 Rust
    // 会进入「Recording 但无原生实例」的悬挂态。
    {
      std::lock_guard<std::mutex> lock(g_recorder_mutex);
      if (g_recorder != recorder) {
        [recorder abandon];
        [[NSFileManager defaultManager] removeItemAtURL:output_url error:nil];
        VNSetError(error_buffer, error_buffer_len, @"录屏启动失败");
        return false;
      }
    }
    return true;
  }
}

/// 停止录屏（异步收尾，结果经 finish callback 回报）。幂等。
void voidnix_record_stop(void) {
  VNRecorder *recorder = VNRecorderRef();
  if (recorder == nil) {
    return;
  }
  [recorder.stream stopCaptureWithCompletionHandler:^(NSError *error) {
    if (error != nil) {
      [recorder finishWithOK:NO error:error.localizedDescription];
    }
    // 正常路径等 recordingOutputDidFinishRecording（文件 moov 落盘）再收尾
  }];
  // 看门狗：文件收尾卡死时保证 UI 状态能收束（Rust 侧另有同量级兜底）。
  // 弱持有：正常收尾后让 recorder/SCStream 立刻释放（系统录屏指示器随采集会话结束），
  // 不因看门狗强引用多挂 15s；对象已释放时消息 nil 是 no-op。
  __weak VNRecorder *weak_recorder = recorder;
  dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(15 * NSEC_PER_SEC)),
                 dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
                   [weak_recorder finishWithOK:NO error:@"结束录屏超时"];
                 });
}

/// 强制拆除原生录屏实例（Rust 侧极端超时兜底）：停流、清单例，不触发 finish 回调。
void voidnix_record_abort(void) {
  VNRecorder *recorder = VNRecorderRef();
  VNClearRecorderIfCurrent(recorder);
  if (recorder != nil) {
    [recorder abandon];
  }
}

}  // extern "C"
