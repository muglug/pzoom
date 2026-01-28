<?php
final class Number1 {
    /** @var string|null */
    private static $zero;

    /**
     * @psalm-pure
     */
    public static function zero(): string {
        self::$zero = "Zero";
        return "hello";
    }
}
