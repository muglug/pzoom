<?php
final class FunctionsList {
    /** @var null|array<string, true> */
    private static ?array $list = null;

    public static function load(): void {
        if (self::$list !== null) {
            return;
        }
        /** @var array<string, true> */
        self::$list = require('/dictionaries/list.php');
    }
}
