<?php
class Globals {
    const SUPER_GLOBALS = [
        '$GLOBALS',
        '$_SERVER',
        '$_GET',
    ];

    /**
     * @param value-of<self::SUPER_GLOBALS>|'$argv'|'$argc' $var_id
     */
    public static function getGlobalType(string $var_id): void {}

    public static function test(): void {
        self::getGlobalType('$GLOBALS');
        self::getGlobalType('$_GET');
        self::getGlobalType('$argv');
        self::getGlobalType('$argc');
    }
}
