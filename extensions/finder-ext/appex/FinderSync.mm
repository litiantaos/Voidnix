#import "FinderSync.h"
#import <pwd.h>
#import <unistd.h>

@implementation FinderSync

- (instancetype)init {
    self = [super init];
    if (self) {
        // Observe "/" so macOS spawns only one extension process.
        // Using "~" causes multiple processes because subdirectories like
        // ~/Library/CloudStorage are separate APFS/network mount points.
        // menuForMenuKind: is called regardless of directoryURLs scope,
        // so all right-click locations still work correctly.
        [FIFinderSyncController defaultController].directoryURLs =
            [NSSet setWithObject:[NSURL fileURLWithPath:@"/"]];
    }
    return self;
}

- (NSString *)commandDir {
    static NSString *dir = nil;
    static dispatch_once_t onceToken;
    dispatch_once(&onceToken, ^{
        // Under App Sandbox, NSHomeDirectory() returns the per-extension
        // container path, not the real user home. We need the real home so
        // the (non-sandboxed) main app and the (sandboxed) extension can
        // share a directory. getpwuid(getuid())->pw_dir bypasses the
        // container redirection. Access is granted by the
        // com.apple.security.temporary-exception.files.home-relative-path
        // entitlement.
        NSString *realHome = nil;
        struct passwd *pw = getpwuid(getuid());
        if (pw && pw->pw_dir) {
            realHome = [NSString stringWithUTF8String:pw->pw_dir];
        }
        if (realHome.length == 0) {
            realHome = NSHomeDirectory();
        }

        dir = [[[[realHome stringByAppendingPathComponent:@"Library"]
                    stringByAppendingPathComponent:@"Application Support"]
                    stringByAppendingPathComponent:@"com.litiantao.voidnix"]
                    stringByAppendingPathComponent:@"commands"];

        NSFileManager *fm = [NSFileManager defaultManager];
        if (![fm fileExistsAtPath:dir]) {
            NSError *err = nil;
            [fm createDirectoryAtPath:dir
          withIntermediateDirectories:YES
                           attributes:nil
                                error:&err];
            if (err) {
                NSLog(@"[VoidnixFinderExt] Failed to create command dir %@: %@",
                      dir, err);
            }
        }
    });
    return dir;
}

- (NSMenu *)menuForMenuKind:(FIMenuKind)whichMenu {
    // Only show custom items in right-click context menus (items or container).
    // Exclude sidebar contextual menus and toolbar item menus.
    if (whichMenu != FIMenuKindContextualMenuForItems
        && whichMenu != FIMenuKindContextualMenuForContainer) return nil;

    // 检查主 app 写入的 enabled 标志文件；未找到则不显示菜单
    NSString *flagPath = [[self commandDir] stringByAppendingPathComponent:@"enabled"];
    if (![[NSFileManager defaultManager] fileExistsAtPath:flagPath]) {
        return nil;
    }

    FIFinderSyncController *controller = [FIFinderSyncController defaultController];
    NSArray<NSURL *> *items = controller.selectedItemURLs;
    BOOL hasItems = items.count > 0;

    NSMenu *menu = [[NSMenu alloc] initWithTitle:@""];

    // Order (fixed across container / items menus so muscle memory works):
    //   1. 拷贝路径            (only when items selected)
    //   2. 在终端中打开        (only when items selected)
    //   3. 新建文件            (always — target dir resolves from the
    //                           Finder window, not the selected item)
    //   4. 显示/不显示隐藏文件  (always — label toggles based on state)
    if (hasItems) {
        NSMenuItem *copyPathItem = [[NSMenuItem alloc]
            initWithTitle:@"拷贝路径"
                   action:@selector(copyPathAction:)
            keyEquivalent:@""];
        copyPathItem.target = self;
        [menu addItem:copyPathItem];

        NSMenuItem *terminalItem = [[NSMenuItem alloc]
            initWithTitle:@"在终端中打开"
                   action:@selector(openTerminalAction:)
            keyEquivalent:@""];
        terminalItem.target = self;
        [menu addItem:terminalItem];
    }

    NSMenuItem *newFileItem = [[NSMenuItem alloc]
        initWithTitle:@"新建文件"
               action:@selector(newFileAction:)
        keyEquivalent:@""];
    newFileItem.target = self;
    [menu addItem:newFileItem];

    NSMenuItem *toggleHiddenItem = [[NSMenuItem alloc]
        initWithTitle:@"切换隐藏文件"
               action:@selector(toggleHiddenAction:)
        keyEquivalent:@""];
    toggleHiddenItem.target = self;
    [menu addItem:toggleHiddenItem];

    return menu;
}

- (void)postCommand:(NSString *)action {
    FIFinderSyncController *controller = [FIFinderSyncController defaultController];
    NSArray<NSURL *> *items = controller.selectedItemURLs;
    NSURL *targetURL = controller.targetedURL;

    NSMutableArray *paths = [NSMutableArray arrayWithCapacity:items.count];
    for (NSURL *url in items ?: @[]) {
        NSString *p = url.path;
        if (p.length > 0) {
            [paths addObject:p];
        }
    }

    NSString *target = targetURL.path ?: @"";
    // Guard: require either paths or target to be non-empty.
    if (paths.count == 0 && target.length == 0) {
        NSLog(@"[VoidnixFinderExt] postCommand:%@ -- no paths or target", action);
        return;
    }

    NSDictionary *cmd = @{
        @"action": action,
        @"paths": paths,
        @"target": target,
    };

    NSError *jsonErr = nil;
    NSData *jsonData = [NSJSONSerialization dataWithJSONObject:cmd options:0 error:&jsonErr];
    if (!jsonData) {
        NSLog(@"[VoidnixFinderExt] JSON serialization error: %@", jsonErr);
        return;
    }

    // Atomic write: write to .tmp first, then rename.  This prevents
    // the Rust watcher from reading a partially-written file.
    NSTimeInterval ts = [[NSDate date] timeIntervalSince1970];
    pid_t pid = [[NSProcessInfo processInfo] processIdentifier];
    NSString *base = [NSString stringWithFormat:@"cmd_%.0f_%d", ts * 1000, pid];
    NSString *tmpPath = [[self commandDir] stringByAppendingPathComponent:
        [base stringByAppendingString:@".tmp"]];
    NSString *finalPath = [[self commandDir] stringByAppendingPathComponent:
        [base stringByAppendingString:@".json"]];

    NSError *writeErr = nil;
    if (![jsonData writeToFile:tmpPath options:NSDataWritingAtomic error:&writeErr]) {
        NSLog(@"[VoidnixFinderExt] Failed to write command file: %@", writeErr);
        return;
    }

    NSFileManager *fm = [NSFileManager defaultManager];
    // Remove stale .json if present (unlikely with pid + timestamp).
    [fm removeItemAtPath:finalPath error:nil];
    if (![fm moveItemAtPath:tmpPath toPath:finalPath error:&writeErr]) {
        NSLog(@"[VoidnixFinderExt] Failed to rename tmp file: %@", writeErr);
        [fm removeItemAtPath:tmpPath error:nil];
    }
}

#pragma mark - Actions

- (void)copyPathAction:(id)sender { [self postCommand:@"copy_path"]; }
- (void)openTerminalAction:(id)sender { [self postCommand:@"open_terminal"]; }
- (void)toggleHiddenAction:(id)sender { [self postCommand:@"toggle_hidden"]; }
- (void)newFileAction:(id)sender { [self postCommand:@"new_file"]; }

@end

// App Extension entry point — Finder Sync extensions require MH_EXECUTE type.
// NSExtensionMain is the standard entry point for all App Extensions,
// provided by the Foundation framework. It loads the principal class
// declared in Info.plist (NSExtensionPrincipalClass).

extern int NSExtensionMain(int argc, const char *argv[]);

int main(int argc, const char *argv[]) {
    return NSExtensionMain(argc, argv);
}