<?php
final class Number1 {
    public static ?string $zero = null;

    /**
     * @psalm-pure
     */
    public static function zero(): ?string {
        return self::$zero;
    }
}
