<?php
abstract class Enum {
    /**
     * @var string[]
     */
    protected const VALUES = [];
    public static function export(): string
    {
        assert(!empty(static::VALUES));
        $values = array_map(
            function(string $val): string {
                return "'" . $val . "'";
            },
            static::VALUES
        );
        return join(",", $values);
    }
}
