<?php
class Fish {
    public static function example(array $vals): void {
        usort($vals, ["self", "compare"]);
    }

    /**
     * @param mixed $a
     * @param mixed $b
     */
    public static function compare($a, $b): int {
        return -1;
    }
}
