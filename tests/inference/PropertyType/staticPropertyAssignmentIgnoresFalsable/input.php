<?php
class EndToEnd {
    private static string $tmpDir = '';
    public static function setUpBeforeClass(): void {
        self::$tmpDir = tempnam(sys_get_temp_dir(), 'PsalmEndToEndTest_');
        assert(self::$tmpDir !== false);
        unlink(self::$tmpDir);
        mkdir(self::$tmpDir);
    }
}
