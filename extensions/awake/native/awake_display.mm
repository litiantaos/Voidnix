#import <Foundation/Foundation.h>
#import <CoreGraphics/CoreGraphics.h>
#import <AppKit/AppKit.h>

@interface CGVirtualDisplayMode : NSObject
- (instancetype)initWithWidth:(NSUInteger)width height:(NSUInteger)height refreshRate:(CGFloat)refreshRate;
@end

@interface CGVirtualDisplaySettings : NSObject
@property(nonatomic) unsigned int hiDPI;
@property(retain, nonatomic) NSArray *modes;
@end

@interface CGVirtualDisplayDescriptor : NSObject
@property(retain, nonatomic) NSString *name;
@property(nonatomic) unsigned int maxPixelsHigh;
@property(nonatomic) unsigned int maxPixelsWide;
@property(nonatomic) CGSize sizeInMillimeters;
@property(nonatomic) unsigned int serialNum;
@property(nonatomic) unsigned int productID;
@property(nonatomic) unsigned int vendorID;
- (void)setDispatchQueue:(dispatch_queue_t)queue;
@end

@interface CGVirtualDisplay : NSObject
@property(readonly, nonatomic) CGDirectDisplayID displayID;
- (instancetype)initWithDescriptor:(CGVirtualDisplayDescriptor *)descriptor;
- (BOOL)applySettings:(CGVirtualDisplaySettings *)settings;
@end

int main(int argc, const char * argv[]) {
    @autoreleasepool {
        BOOL mirrorMode = NO;
        if (argc > 1 && strcmp(argv[1], "--mirror") == 0) {
            mirrorMode = YES;
        }

        NSBundle *bundle = [NSBundle bundleWithPath:@"/System/Library/Frameworks/CoreDisplay.framework"];
        if (![bundle load]) {
            NSLog(@"Failed to load CoreDisplay.framework");
            return 1;
        }

        Class CGVirtualDisplayDescriptorClass = NSClassFromString(@"CGVirtualDisplayDescriptor");
        Class CGVirtualDisplaySettingsClass = NSClassFromString(@"CGVirtualDisplaySettings");
        Class CGVirtualDisplayModeClass = NSClassFromString(@"CGVirtualDisplayMode");
        Class CGVirtualDisplayClass = NSClassFromString(@"CGVirtualDisplay");

        if (!CGVirtualDisplayDescriptorClass || !CGVirtualDisplaySettingsClass || !CGVirtualDisplayModeClass || !CGVirtualDisplayClass) {
            NSLog(@"Failed to load private classes");
            return 1;
        }

        CGVirtualDisplayDescriptor *descriptor = [[CGVirtualDisplayDescriptorClass alloc] init];
        descriptor.name = @"Display Wakelock";
        descriptor.maxPixelsWide = 1920;
        descriptor.maxPixelsHigh = 1080;
        descriptor.sizeInMillimeters = CGSizeMake(500, 300);
        descriptor.serialNum = 1;
        descriptor.productID = 1;
        descriptor.vendorID = 1;
        [descriptor setDispatchQueue:dispatch_get_global_queue(QOS_CLASS_USER_INTERACTIVE, 0)];

        CGVirtualDisplay *display = [[CGVirtualDisplayClass alloc] initWithDescriptor:descriptor];

        CGVirtualDisplayMode *mode = [[CGVirtualDisplayModeClass alloc] initWithWidth:1920 height:1080 refreshRate:60.0];

        CGVirtualDisplaySettings *settings = [[CGVirtualDisplaySettingsClass alloc] init];
        settings.hiDPI = 1;
        settings.modes = @[mode];

        BOOL success = [display applySettings:settings];
        if (!success) {
            NSLog(@"Failed to apply virtual display settings");
            return 1;
        }

        {
            CGDirectDisplayID virtualID = display.displayID;
            if (virtualID != kCGNullDirectDisplay) {
                CGDisplayConfigRef config = NULL;
                CGError err = CGBeginDisplayConfiguration(&config);
                if (err == kCGErrorSuccess && config != NULL) {
                    if (mirrorMode) {
                        CGConfigureDisplayMirrorOfDisplay(config, virtualID, CGMainDisplayID());
                    } else {
                        CGConfigureDisplayMirrorOfDisplay(config, virtualID, kCGNullDirectDisplay);
                    }
                    err = CGCompleteDisplayConfiguration(config, kCGConfigureForSession);
                    if (err != kCGErrorSuccess) {
                        NSLog(@"Warning: failed to configure display: %d", err);
                    }
                } else {
                    NSLog(@"Warning: failed to begin display configuration: %d", err);
                }
            }
        }

        printf("READY\n");
        fflush(stdout);

        char buffer[256];
        while (fgets(buffer, sizeof(buffer), stdin) != NULL) {
        }
    }
    return 0;
}
