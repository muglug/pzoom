<?php
class Foo {
    public const DAYS = [
        1 => "mon",
        2 => "tue",
        3 => "wed",
        4 => "thu",
        5 => "fri",
        6 => "sat",
        7 => "sun",
    ];

    /** @param key-of<self::DAYS> $dayNum*/
    private static function doGetDayName(int $dayNum): string {
        return self::DAYS[$dayNum];
    }

    /** @throws LogicException */
    public static function getDayName(int $dayNum): string {
        if (! array_key_exists($dayNum, self::DAYS)) {
            throw new \LogicException();
        }
        return self::doGetDayName($dayNum);
    }
}
