<?php
final class H10 {
    /** @var non-empty-array<string, string>|null */
    private static ?array $call_map = null;

    /** @return non-empty-array<string, string> */
    public static function load(): array {
        /** @var non-empty-array<string, string> */
        $call_map = ['a' => 'b'];
        self::$call_map = $call_map;
        assert(!empty(self::$call_map));
        return self::$call_map;
    }
}
