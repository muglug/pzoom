<?php
trait T {
    /**
     * @var string|null
     */
    private static $b;

    private static function booA(): string {
        if (self::$b === null) {
            return "hello";
        }
        return self::$b;
    }
}

class C {
    use T;
}
